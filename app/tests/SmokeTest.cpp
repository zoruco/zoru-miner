#include "Window.h"
#include <QtTest>
#include <QFile>
#include <QJsonDocument>
#include <QElapsedTimer>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>

class SmokeTest : public QObject {
    Q_OBJECT
    QJsonObject configuration() {
        QFile input(qEnvironmentVariable("ZORU_GUI_TEST_CONFIG"));
        if (!input.open(QIODevice::ReadOnly)) return {};
        auto options = QJsonDocument::fromJson(input.readAll()).object();
        // Credentials stay in memory. A private inherited pipe avoids putting
        // a real wallet password in the fixture file, environment or argv.
        static QString password;
        static bool loaded = false;
        if (!loaded) {
            loaded = true;
            bool valid = false;
            const int descriptor = qEnvironmentVariableIntValue("ZORU_GUI_TEST_PASSWORD_FD", &valid);
            if (valid && descriptor >= 3) {
                QFile pipe;
                if (pipe.open(descriptor, QIODevice::ReadOnly, QFileDevice::DontCloseHandle)) {
                    auto line = pipe.readLine(16385);
                    if (line.endsWith('\n') && line.size() <= 16384) {
                        line.chop(1);
                        if (line.endsWith('\r')) line.chop(1);
                        password = QString::fromUtf8(line);
                    }
                    line.fill('\0');
                }
            }
        }
        // Never read passwords persisted in JSON, including old fixtures.
        options.remove("password");
        options["password"] = password;
        return options;
    }
private slots:
    void disconnectedControls() {
        Window window("zoru-miner-cli"); window.show();
        QVERIFY(window.state().isEmpty());
        QVERIFY(!window.findChild<QPushButton*>("mine")->isEnabled());
        QVERIFY(!window.findChild<QPushButton*>("claim")->isEnabled());
        QVERIFY(window.findChild<QPushButton*>("connect")->isEnabled());
    }
    void backendHandshake_data() {
        QTest::addColumn<bool>("fail");
        QTest::newRow("inspection-failed") << true;
        QTest::newRow("inspection-succeeded") << false;
    }
    void backendHandshake() {
#if defined(Q_OS_UNIX)
        QFETCH(bool, fail);
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        QFile script(directory.filePath("mock-backend.sh"));
        QVERIFY(script.open(QIODevice::WriteOnly | QIODevice::Truncate));
        QByteArray body = R"(#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"action":"connect"'*)
      printf '%s\n' '{"event":"state","data":{"status":"connected","address":"0x1111111111111111111111111111111111111111"}}'
)";
        if (fail) {
            body += R"(      printf '%s\n' '{"event":"error","error":"Simulated inspection failure"}'
)";
        } else {
            body += R"(      printf '%s\n' '{"event":"inspection","data":{"chainId":6736,"contracts":{},"allocatedSupplyBaseUnits":"49500000000","totalSupplyBaseUnits":"500000000","globalPendingBaseUnits":"49000000000","globalUnclaimedBaseUnits":"0","globalClaimedBaseUnits":"500000000","totalAcceptedProofs":"401","currentEpoch":"71","admissionPaused":false,"claimsPaused":false,"myPendingBaseUnits":"250000000","myUnclaimedBaseUnits":"500000000","myClaimedBaseUnits":"250000000","myRewardsHistoryComplete":true}}'
)";
        }
        body += R"(      printf '%s\n' '{"event":"command-complete","action":"connect"}'
      ;;
    *'"action":"quit"'*) exit 0 ;;
  esac
done
)";
        QCOMPARE(script.write(body), body.size());
        script.close();
        QVERIFY(QFile::setPermissions(script.fileName(), QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner));
        Window window(script.fileName());
        bool completed = false;
        connect(&window, &Window::backendEvent, &window, [&](const QJsonObject &event) {
            if (event["event"] == "command-complete" && event["action"] == "connect") completed = true;
        });
        window.show();
        QTest::mouseClick(window.findChild<QPushButton*>("connect"), Qt::LeftButton);
        QTRY_VERIFY_WITH_TIMEOUT(completed, 10000);
        QVERIFY(window.findChild<QPushButton*>("connect")->isEnabled());
        QCOMPARE(window.findChild<QPushButton*>("mine")->isEnabled(), !fail);
        QCOMPARE(window.findChild<QPushButton*>("claim")->isEnabled(), !fail);
        QCOMPARE(window.findChild<QPushButton*>("refresh")->isEnabled(), !fail);
        QCOMPARE(window.state().isEmpty(), fail);
        if (fail) QCOMPARE(window.lastError(), QString("Simulated inspection failure"));
        else {
            QCOMPARE(window.findChild<QLabel*>("globalPending")->text(), QString("490.00000000 ZORU"));
            QCOMPARE(window.findChild<QLabel*>("globalUnclaimed")->text(), QString("0.00000000 ZORU"));
            QCOMPARE(window.findChild<QLabel*>("globalClaimed")->text(), QString("5.00000000 ZORU"));
            QCOMPARE(window.findChild<QLabel*>("globalMinted")->text(), QString("5.00000000 ZORU"));
            QCOMPARE(window.findChild<QLabel*>("myPending")->text(), QString("2.50000000 ZORU"));
            QCOMPARE(window.findChild<QLabel*>("myUnclaimed")->text(), QString("5.00000000 ZORU"));
            QCOMPARE(window.findChild<QLabel*>("myClaimed")->text(), QString("2.50000000 ZORU"));
            QVERIFY(window.findChild<QLabel*>("networkInfo")->text().contains("401 accepted proofs"));
        }
        window.close();
#else
        QSKIP("Uses a POSIX shell fixture.");
#endif
    }
    void realReadOnlyMainnetHandshake() {
        const auto path = qEnvironmentVariable("ZORU_GUI_READONLY_CONFIG");
        if (path.isEmpty()) QSKIP("Set ZORU_GUI_READONLY_CONFIG to a mainnet fixture without wallet credentials.");
        QFile input(path);
        QVERIFY2(input.open(QIODevice::ReadOnly), "Cannot read the read-only mainnet configuration.");
        const auto options = QJsonDocument::fromJson(input.readAll()).object();
        QVERIFY(!options.isEmpty());
        QVERIFY(!options["backend"].toString().isEmpty());
        QVERIFY(options["wallet"].toString().isEmpty());
        Window window(options["backend"].toString());
        window.configure(options);
        int inspections = 0;
        int completedConnect = 0;
        int completedInspect = 0;
        connect(&window, &Window::backendEvent, &window, [&](const QJsonObject &event) {
            if (event["event"] == "inspection") ++inspections;
            if (event["event"] == "command-complete") {
                if (event["action"] == "connect") ++completedConnect;
                if (event["action"] == "inspect") ++completedInspect;
            }
        });
        window.show();
        QTest::mouseClick(window.findChild<QPushButton*>("connect"), Qt::LeftButton);
        QTRY_VERIFY_WITH_TIMEOUT(completedConnect > 0, 60000);
        QCOMPARE(window.lastError(), QString());
        QCOMPARE(window.state().value("status").toString(), QString("connected"));
        QCOMPARE(window.state().value("chainId").toInt(), 6736);
        QVERIFY(inspections > 0);
        QVERIFY(window.findChild<QPushButton*>("refresh")->isEnabled());
        QVERIFY(!window.findChild<QPushButton*>("mine")->isEnabled());
        QVERIFY(!window.findChild<QPushButton*>("claim")->isEnabled());
        QTest::mouseClick(window.findChild<QPushButton*>("refresh"), Qt::LeftButton);
        QTRY_VERIFY_WITH_TIMEOUT(completedInspect > 0, 60000);
        QVERIFY(inspections >= 2);
        QCOMPARE(window.lastError(), QString());
        QCOMPARE(window.state().value("status").toString(), QString("connected"));
        window.close();
    }
    void realBackendMiningAndClaim() {
        const auto config = qEnvironmentVariable("ZORU_GUI_TEST_CONFIG");
        if (config.isEmpty()) QSKIP("Set ZORU_GUI_TEST_CONFIG to a private disposable fixture configuration.");
        auto options = configuration(); QVERIFY(!options.isEmpty());
        QVERIFY2(!options["password"].toString().isEmpty(), "Provide the wallet password through ZORU_GUI_TEST_PASSWORD_FD.");
        QString previousStatus;
        int previousAccepted = -1;
        Window window(options["backend"].toString()); window.configure(options); window.show();
        connect(&window, &Window::backendEvent, this, [&](const QJsonObject &event){
            if (event["event"] == "error") qWarning().noquote() << event["error"].toString();
            if (event["event"] == "command-complete") qInfo().noquote() << "completed" << event["action"].toString();
            if (event["event"] == "state") {
                const auto state = event["data"].toObject();
                const auto status = state["status"].toString();
                const auto accepted = state["accepted"].toInt();
                if (status != previousStatus || accepted != previousAccepted) {
                    qInfo().noquote() << "state" << status << "accepted" << accepted
                        << "hashes" << state["hashes"].toVariant().toString()
                        << "resource pauses" << state["resourcePauses"].toInt();
                    previousStatus = status; previousAccepted = accepted;
                }
            }
        });
        qInfo() << "phase: connect";
        QTest::mouseClick(window.findChild<QPushButton*>("connect"), Qt::LeftButton);
        QTRY_COMPARE_WITH_TIMEOUT(window.state()["status"].toString(), QString("connected"), 60000);
        QTRY_VERIFY_WITH_TIMEOUT(window.findChild<QPushButton*>("mine")->isEnabled(), 60000);
        const auto initialPaid = window.state()["rewardPaidBaseUnits"].toString("0");
        qInfo() << "phase: mine";
        QTest::mouseClick(window.findChild<QPushButton*>("mine"), Qt::LeftButton);
        QElapsedTimer mining, resourcePause;
        mining.start();
        while (window.state()["accepted"].toInt() == 0 && mining.elapsed() < 120000) {
            QTest::qWait(100);
            if (window.state()["status"] == "paused-resource") {
                if (!resourcePause.isValid()) resourcePause.start();
                if (resourcePause.elapsed() >= 10000) {
                    QVERIFY(window.findChild<QLabel*>("status")->text().contains("busy"));
                    window.send({{"action","stop"}});
                    QTRY_COMPARE_WITH_TIMEOUT(window.state()["status"].toString(), QString("stopped"), 15000);
                    QSKIP("BLOCKED_RESOURCE: the host resource guard paused mining; use an idle test host.");
                }
            } else resourcePause.invalidate();
        }
        QVERIFY2(window.state()["accepted"].toInt() > 0,
                 qPrintable("No accepted proof before the bounded mining deadline. State: " + previousStatus + "; " + window.lastError()));
        qInfo() << "phase: pause/resume";
        QTest::mouseClick(window.findChild<QPushButton*>("pause"), Qt::LeftButton);
        QTRY_COMPARE_WITH_TIMEOUT(window.state()["status"].toString(), QString("paused"), 15000);
        QTest::mouseClick(window.findChild<QPushButton*>("pause"), Qt::LeftButton);
        QTRY_COMPARE_WITH_TIMEOUT(window.state()["status"].toString(), QString("mining"), 20000);
        QTest::mouseClick(window.findChild<QPushButton*>("stop"), Qt::LeftButton);
        QTRY_COMPARE_WITH_TIMEOUT(window.state()["status"].toString(), QString("stopped"), 60000);
        QTRY_VERIFY_WITH_TIMEOUT(window.findChild<QPushButton*>("claim")->isEnabled(), 60000);
        // Wait on wall time only: do not manipulate shared network time.
        qInfo() << "phase: wait for epoch maturity";
        QTest::qWait(65000);
        qInfo() << "phase: claim";
        QTest::mouseClick(window.findChild<QPushButton*>("claim"), Qt::LeftButton);
        QTRY_VERIFY_WITH_TIMEOUT(window.state()["rewardPaidBaseUnits"].toString("0") != initialPaid, 90000);
        QTRY_VERIFY_WITH_TIMEOUT(window.findChild<QPushButton*>("connect")->isEnabled(), 60000);
        const auto paid = window.state()["rewardPaidBaseUnits"].toString();
        QVERIFY(window.findChild<QTableWidget*>("proofs")->rowCount() > 0);
        window.grab().save(options["screenshot"].toString());
        // Reconnect with the same wallet/history through the actual transport.
        qInfo() << "phase: reconnect";
        window.configure(options);
        QTest::mouseClick(window.findChild<QPushButton*>("connect"), Qt::LeftButton);
        QTRY_COMPARE_WITH_TIMEOUT(window.state()["status"].toString(), QString("connected"), 60000);
        QCOMPARE(window.state()["rewardPaidBaseUnits"].toString(), paid);
        QTRY_VERIFY_WITH_TIMEOUT(window.findChild<QPushButton*>("claim")->isEnabled(), 60000);
        window.close();
    }
    void invalidRpcAndWrongChain() {
        const auto config = qEnvironmentVariable("ZORU_GUI_TEST_CONFIG");
        if (config.isEmpty()) QSKIP("Requires disposable GUI configuration.");
        auto options = configuration(); QVERIFY(!options.isEmpty());
        options["read"] = "http://127.0.0.1:1";
        Window invalid(options["backend"].toString()); invalid.configure(options); invalid.show();
        QTest::mouseClick(invalid.findChild<QPushButton*>("connect"), Qt::LeftButton);
        QTRY_VERIFY_WITH_TIMEOUT(!invalid.lastError().isEmpty(), 30000);
        QVERIFY(!invalid.findChild<QPushButton*>("mine")->isEnabled()); invalid.close();
        QTcpServer server; QVERIFY(server.listen(QHostAddress::LocalHost, 0));
        connect(&server, &QTcpServer::newConnection, &server, [&]{
            auto *socket = server.nextPendingConnection();
            connect(socket, &QTcpSocket::readyRead, socket, [socket]{
                socket->readAll(); QByteArray body = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1a50\"}";
                socket->write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: " + QByteArray::number(body.size()) + "\r\nConnection: close\r\n\r\n" + body); socket->disconnectFromHost();
            });
            connect(socket, &QTcpSocket::disconnected, socket, &QObject::deleteLater);
        });
        options["read"] = QString("http://127.0.0.1:%1").arg(server.serverPort());
        Window wrong(options["backend"].toString()); wrong.configure(options); wrong.show();
        QTest::mouseClick(wrong.findChild<QPushButton*>("connect"), Qt::LeftButton);
        QTRY_VERIFY_WITH_TIMEOUT(wrong.lastError().contains("expected 6736"), 30000);
        QVERIFY(!wrong.findChild<QPushButton*>("mine")->isEnabled()); wrong.close();
    }
};
QTEST_MAIN(SmokeTest)
#include "SmokeTest.moc"
