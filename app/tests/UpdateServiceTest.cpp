#include "UpdateService.h"
#include "AppVersion.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QFile>
#include <QJsonDocument>
#include <QSignalSpy>
#include <QSslCertificate>
#include <QSslConfiguration>
#include <QSslKey>
#include <QSslSocket>
#include <QTcpServer>
#include <QTemporaryDir>
#include <QtTest>
#include <atomic>

class TlsFixture final : public QTcpServer {
  public:
    QSslCertificate certificate;
    QSslKey key;
    QByteArray metadata, package;
    bool corrupt = false, stall = false, unavailable = false;
    int checks = 0;

  protected:
    void incomingConnection(qintptr descriptor) override {
        auto *socket = new QSslSocket(this);
        socket->setSocketDescriptor(descriptor);
        socket->setLocalCertificate(certificate);
        socket->setPrivateKey(key);
        socket->setPeerVerifyMode(QSslSocket::VerifyNone);
        connect(socket, &QSslSocket::disconnected, socket, &QObject::deleteLater);
        connect(socket, &QSslSocket::readyRead, this, [this, socket] {
            if (socket->property("answered").toBool())
                return;
            auto request = socket->property("request").toByteArray() + socket->readAll();
            socket->setProperty("request", request);
            if (!request.contains("\r\n\r\n"))
                return;
            socket->setProperty("answered", true);
            const bool info = request.split(' ').value(1) == "/update.json";
            if (info)
                ++checks;
            if (info && unavailable) {
                socket->write(
                    "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                socket->disconnectFromHost();
                return;
            }
            QByteArray body = info ? metadata : package;
            if (!info && corrupt && !body.isEmpty())
                body[0] = body[0] ^ 1;
            socket->write("HTTP/1.1 200 OK\r\nContent-Length: " + QByteArray::number(body.size()) +
                          "\r\nConnection: close\r\n\r\n");
            if (!info && stall) {
                socket->write(body.left(10));
                return;
            }
            socket->write(body);
            socket->disconnectFromHost();
        });
        socket->startServerEncryption();
    }
};

class UpdateServiceTest final : public QObject {
    Q_OBJECT
    QTemporaryDir directory;
    TlsFixture server;
    QString helper;
    void configure(UpdateService &service, const QString &name) {
        service.initialize(directory.filePath(name), false);
        service.m_enabled = true;
        service.m_helper = helper;
        service.m_folder = directory.filePath(name + "/downloads");
        service.m_feed =
            QUrl("https://127.0.0.1:" + QString::number(server.serverPort()) + "/update.json");
        server.metadata =
            QJsonDocument(
                QJsonObject{
                    {"ok", true},
                    {"available", true},
                    {"version", "9.0.0"},
                    {"notes", "A test release."},
                    {"artifact",
                     QJsonObject{{"fileName", "ZORU-Miner-9.0.0-Linux-amd64.deb"},
                                 {"url", "https://127.0.0.1:" +
                                             QString::number(server.serverPort()) + "/package"},
                                 {"size", server.package.size()},
                                 {"sha256", QString::fromLatin1(
                                                QCryptographicHash::hash(server.package,
                                                                         QCryptographicHash::Sha256)
                                                    .toHex())}}}})
                .toJson();
        server.corrupt = false;
        server.stall = false;
        server.unavailable = false;
    }
  private slots:
    void initTestCase() {
#ifndef Q_OS_UNIX
        QSKIP("The local TLS process fixture runs on Unix.");
#else
        QVERIFY(directory.isValid());
        QCoreApplication::setApplicationVersion(QStringLiteral(ZORU_APP_VERSION));
        const QString cert = directory.filePath("tls.crt"), key = directory.filePath("tls.key");
        QProcess openssl;
        openssl.start("openssl", {"req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                                  "-subj", "/CN=127.0.0.1", "-addext",
                                  "subjectAltName=IP:127.0.0.1", "-keyout", key, "-out", cert});
        QVERIFY(openssl.waitForFinished(15000));
        QCOMPARE(openssl.exitCode(), 0);
        QFile certificate(cert), privateKey(key);
        QVERIFY(certificate.open(QIODevice::ReadOnly));
        QVERIFY(privateKey.open(QIODevice::ReadOnly));
        server.certificate = QSslCertificate(certificate.readAll());
        server.key = QSslKey(privateKey.readAll(), QSsl::Rsa);
        auto ssl = QSslConfiguration::defaultConfiguration();
        ssl.addCaCertificate(server.certificate);
        QSslConfiguration::setDefaultConfiguration(ssl);
        QVERIFY(server.listen(QHostAddress::LocalHost));
        server.package = QByteArray(65536, 'x');
        helper = directory.filePath("test-update-tool");
        QFile script(helper);
        QVERIFY(script.open(QIODevice::WriteOnly));
        script.write(R"PY(#!/usr/bin/python3
import hashlib,json,pathlib,sys
mode=sys.argv[1];args=sys.argv[2:];metadata=pathlib.Path(args[args.index('--metadata')+1]);value=json.loads(metadata.read_text())
if mode=='verify':
 path=pathlib.Path(args[args.index('--package')+1]);raw=path.read_bytes();artifact=value['artifact']
 ok=len(raw)==artifact['size'] and hashlib.sha256(raw).hexdigest()==artifact['sha256']
 print(json.dumps({'ok':ok,'verified':ok,'version':value['version']}));sys.exit(0 if ok else 1)
print(json.dumps(value))
)PY");
        script.close();
        QVERIFY(script.setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner));
#endif
    }
    void download_is_not_ready_before_verification_and_parallel_checks_are_ignored() {
        UpdateService service;
        configure(service, "complete");
        QSignalSpy available(&service, &UpdateService::available);
        const int before = server.checks;
        service.check();
        service.check();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Available"), 10000);
        QCOMPARE(server.checks, before + 1);
        QCOMPARE(available.size(), 1);
        QVERIFY(service.canDownload());
        service.download();
        QVERIFY(!service.canInstall());
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Ready"), 10000);
        QFile file(service.downloadFile());
        QVERIFY(file.open(QIODevice::ReadOnly));
        QCOMPARE(file.readAll(), server.package);
    }
    void checksum_failure_removes_the_download_and_retry_can_succeed() {
        UpdateService service;
        configure(service, "checksum");
        service.check();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Available"), 10000);
        server.corrupt = true;
        service.download();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Error"), 10000);
        QVERIFY(!QFileInfo::exists(
            directory.filePath("checksum/downloads/ZORU-Miner-9.0.0-Linux-amd64.deb")));
        QVERIFY(service.canDownload());
        server.corrupt = false;
        service.download();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Ready"), 10000);
    }
    void cancelled_download_does_not_leave_an_installable_partial_file() {
        UpdateService service;
        configure(service, "cancel");
        service.check();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Available"), 10000);
        server.stall = true;
        service.download();
        QTRY_VERIFY_WITH_TIMEOUT(service.m_received > 0, 10000);
        service.cancelDownload();
        QCOMPARE(service.state(), QString("Available"));
        QVERIFY(!QFileInfo::exists(service.downloadFile()));
        QVERIFY(!service.canInstall());
    }
    void unavailable_feed_never_claims_that_the_app_is_up_to_date() {
        UpdateService service;
        configure(service, "unavailable");
        server.unavailable = true;
        service.check();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Error"), 10000);
        QVERIFY(!service.canDownload());
        QVERIFY(!service.canInstall());
    }
    void preparing_install_blocks_background_checks_and_can_be_cancelled() {
        UpdateService service;
        configure(service, "preparing");
        service.setState("Preparing", "Preparing installation");
        const int before = server.checks;
        service.check();
        QCOMPARE(service.state(), QString("Preparing"));
        QCOMPARE(server.checks, before);
        service.cancelPreparedInstall();
        QCOMPARE(service.state(), QString("Error"));
        QVERIFY(!service.installing());
    }
    void oversized_response_is_cancelled_without_reading_a_closed_device_and_can_retry() {
        UpdateService service;
        configure(service, "oversized");
        const auto valid = server.metadata;
        static std::atomic<int> invalidReads{0};
        invalidReads = 0;
        struct RestoreHandler {
            QtMessageHandler previous;
            ~RestoreHandler() { qInstallMessageHandler(previous); }
        } restore{qInstallMessageHandler(
            [](QtMsgType, const QMessageLogContext &, const QString &message) {
                if (message.contains("QIODevice::read"))
                    ++invalidReads;
            })};
        server.metadata = QByteArray(70000, 'x');
        service.check();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Error"), 10000);
        QVERIFY(!service.busy());
        QVERIFY(!service.canInstall());
        QCOMPARE(invalidReads.load(), 0);
        server.metadata = valid;
        service.check();
        QTRY_COMPARE_WITH_TIMEOUT(service.state(), QString("Available"), 10000);
    }
};
QTEST_GUILESS_MAIN(UpdateServiceTest)
#include "UpdateServiceTest.moc"
