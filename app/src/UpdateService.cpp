#include "UpdateService.h"
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QNetworkReply>
#include <QStandardPaths>
#include <QUuid>

UpdateService::UpdateService(QObject *parent) : QObject(parent), m_network(this) {
    m_deadline.setSingleShot(true);
    connect(&m_deadline, &QTimer::timeout, this, [this] {
        if (m_reply)
            m_reply->abort();
    });
    m_periodic.setInterval(6 * 60 * 60 * 1000);
    connect(&m_periodic, &QTimer::timeout, this, [this] {
        if (m_automatic && m_enabled && !busy())
            check();
    });
    connect(&m_process, &QProcess::readyReadStandardOutput, this, [this] {
        m_output.append(m_process.readAllStandardOutput());
        if (m_output.size() > 65536) {
            m_output.truncate(65536);
            if (m_operation != "install") {
                m_process.kill();
                fail("The update information could not be read.");
            }
        }
    });
    connect(&m_process, &QProcess::readyReadStandardError, this,
            [this] { m_process.readAllStandardError(); });
    connect(&m_process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished), this,
            &UpdateService::processFinished);
    connect(&m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart)
            return;
        if (m_operation == "install")
            setState("Ready",
                     "Installation could not start. Your download is kept; you can try again.");
        else
            fail("The update tool could not start. Reinstall ZORU Miner and try again.");
    });
}
UpdateService::~UpdateService() {
    ++m_generation;
    m_deadline.stop();
    m_periodic.stop();
    if (m_reply) {
        disconnect(m_reply, nullptr, this, nullptr);
        m_reply->abort();
    }
    if (m_process.state() != QProcess::NotRunning && !installing()) {
        m_process.kill();
        m_process.waitForFinished(1000);
    }
}
void UpdateService::initialize(const QString &storage, bool networkEnabled) {
    m_storage = QDir(storage).filePath("updates");
    QDir().mkpath(m_storage);
    QFile::setPermissions(m_storage, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
    m_preferences =
        std::make_unique<QSettings>(QDir(storage).filePath("updates.ini"), QSettings::IniFormat);
    m_automatic = m_preferences->value("automaticChecks", true).toBool();
    m_folder = QDir(QStandardPaths::writableLocation(QStandardPaths::DownloadLocation))
                   .filePath("ZORU Miner");
    m_feed = QUrl("https://github.com/zoruco/zoru-miner/releases/latest/download/update.json");
#ifdef Q_OS_WIN
    m_helper = QDir(QCoreApplication::applicationDirPath()).filePath("zoru-update.exe");
#else
    m_helper = QDir(QCoreApplication::applicationDirPath()).filePath("zoru-update");
#endif
    m_enabled = networkEnabled;
#ifdef Q_OS_WIN
    for (const auto &name : QDir(m_storage).entryList({"update-runner-*.exe"}, QDir::Files))
        QFile::remove(QDir(m_storage).filePath(name));
#endif
    if (m_enabled) {
        m_periodic.start();
        QTimer::singleShot(10000, this, [this] {
            if (m_automatic && !busy())
                check();
        });
    }
    emit changed();
}
QNetworkRequest UpdateService::request(const QUrl &url) const {
    QNetworkRequest r(url);
    r.setHeader(QNetworkRequest::UserAgentHeader,
                "ZORU-Miner/" + QCoreApplication::applicationVersion());
    r.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                   QNetworkRequest::NoLessSafeRedirectPolicy);
    r.setMaximumRedirectsAllowed(5);
    r.setTransferTimeout(30000);
    return r;
}
void UpdateService::setState(const QString &state, const QString &message) {
    m_state = state;
    m_message = message;
    emit changed();
}
void UpdateService::fail(const QString &message) {
    m_deadline.stop();
    if (m_file) {
        m_file->cancelWriting();
        m_file.reset();
    }
    setState("Error", message);
}
void UpdateService::setAutomaticChecks(bool enabled) {
    m_automatic = enabled;
    if (m_preferences) {
        m_preferences->setValue("automaticChecks", enabled);
        m_preferences->sync();
    }
    emit changed();
}
void UpdateService::check() {
    if (busy() || installing())
        return;
    if (!m_enabled) {
        setState("Error", "Update checks are unavailable in this session.");
        return;
    }
    if (m_feed.scheme() != "https" || !QFileInfo(m_helper).isExecutable()) {
        fail("The update service is unavailable. Try again later.");
        return;
    }
    m_releaseValid = false;
    m_artifact = {};
    m_metadata.clear();
    m_download.clear();
    const auto generation = ++m_generation;
    setState("Checking", "Checking for updates…");
    m_reply = m_network.get(request(m_feed));
    const auto reply = m_reply;
    reply->setReadBufferSize(65537);
    emit changed();
    connect(reply, &QIODevice::readyRead, this, [this, reply, generation] {
        if (generation != m_generation)
            return;
        m_metadata.append(reply->readAll());
        if (m_metadata.size() > 65536)
            reply->abort();
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply, generation] {
        reply->deleteLater();
        if (generation != m_generation)
            return;
        m_deadline.stop();
        m_reply = nullptr;
        if (reply->isOpen())
            m_metadata.append(reply->readAll());
        if (reply->error() != QNetworkReply::NoError || reply->url().scheme() != "https" ||
            m_metadata.isEmpty() || m_metadata.size() > 65536) {
            fail("Could not check for updates. Try again later.");
            return;
        }
        QSaveFile file(QDir(m_storage).filePath("metadata.json"));
        if (!file.open(QIODevice::WriteOnly) || file.write(m_metadata) != m_metadata.size() ||
            !file.commit()) {
            fail("The update information could not be saved.");
            return;
        }
        QFile::setPermissions(file.fileName(), QFile::ReadOwner | QFile::WriteOwner);
        verify("check");
    });
    m_deadline.start(30000);
}
void UpdateService::verify(const QString &operation) {
    m_operation = operation;
    m_output.clear();
    QStringList args = {operation, "--metadata", QDir(m_storage).filePath("metadata.json"),
                        "--current-version", QCoreApplication::applicationVersion()};
    if (operation == "verify")
        args << "--package" << m_download;
    m_process.setProgram(m_helper);
    m_process.setArguments(args);
    m_process.start();
    emit changed();
}
void UpdateService::processFinished(int code, QProcess::ExitStatus status) {
    m_output.append(m_process.readAllStandardOutput());
    const auto result = QJsonDocument::fromJson(m_output).object();
    if (code != 0 || status != QProcess::NormalExit || !result.value("ok").toBool()) {
        if (m_operation == "install")
            setState("Ready",
                     "Installation did not finish. Your download is kept; you can try again.");
        else if (m_operation == "verify") {
            QFile::remove(m_download);
            m_download.clear();
            fail("The download could not be verified. Download it again.");
        } else
            fail("The update information could not be verified. Try again later.");
        return;
    }
    if (m_operation == "check") {
        if (!result.value("available").isBool() || !result.value("version").isString() ||
            result.value("version").toString().isEmpty() || !result.value("notes").isString()) {
            fail("The update information could not be read. Try again later.");
            return;
        }
        m_version = result.value("version").toString();
        m_notes = result.value("notes").toString();
        if (result.value("status").toString() == "unsupported") {
            setState("Unavailable", "No update package is available for this operating system.");
            return;
        }
        if (!result.value("available").toBool()) {
            setState("UpToDate", "You have the latest version of ZORU Miner.");
            return;
        }
        m_artifact = result.value("artifact").toObject();
        m_releaseValid = !m_artifact.isEmpty();
        if (!m_releaseValid) {
            fail("No update is available for this operating system.");
            return;
        }
        setState("Available", "ZORU Miner " + m_version + " is available.");
        emit available(m_version);
    } else if (m_operation == "verify") {
        if (!result.value("verified").toBool() || result.value("version").toString() != m_version) {
            QFile::remove(m_download);
            fail("The download could not be verified. Download it again.");
            return;
        }
        setState("Ready", "The update is ready to install.");
    } else if (m_operation == "install") {
        if (!result.value("installed").toBool()) {
            setState("Ready", "Installation did not finish. You can try again.");
            return;
        }
        setState("Restarting", "Update installed. Reopening ZORU Miner…");
        if (QProcess::startDetached(
                m_helper,
                {"restart", "--parent", QString::number(QCoreApplication::applicationPid())}))
            emit restartReady();
        else
            setState("UpToDate", "Update installed. Close and open ZORU Miner again.");
    }
}
void UpdateService::download() {
    if (!canDownload())
        return;
    const QString name = m_artifact.value("fileName").toString();
    const QUrl url(m_artifact.value("url").toString());
    const qint64 total = m_artifact.value("size").toVariant().toLongLong();
    if (name.isEmpty() || QFileInfo(name).fileName() != name || url.scheme() != "https" ||
        total <= 0 || total > 536870912) {
        fail("The update download is invalid.");
        return;
    }
    if (!QDir().mkpath(m_folder)) {
        fail("The download folder could not be created.");
        return;
    }
    m_download = QDir(m_folder).filePath(name);
    m_received = 0;
    m_total = total;
    m_file = std::make_unique<QSaveFile>(m_download);
    m_file->setDirectWriteFallback(false);
    if (!m_file->open(QIODevice::WriteOnly)) {
        m_file.reset();
        fail("The update file could not be saved.");
        return;
    }
    const auto generation = ++m_generation;
    setState("Downloading", "Downloading ZORU Miner " + m_version + "…");
    m_reply = m_network.get(request(url));
    const auto reply = m_reply;
    reply->setReadBufferSize(65536);
    emit changed();
    connect(reply, &QIODevice::readyRead, this, [this, reply, generation] {
        if (generation != m_generation || !m_file)
            return;
        const auto bytes = reply->readAll();
        m_received += bytes.size();
        if (m_received > m_total || m_file->write(bytes) != bytes.size()) {
            reply->abort();
            return;
        }
        emit changed();
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply, generation] {
        reply->deleteLater();
        if (generation != m_generation)
            return;
        m_deadline.stop();
        m_reply = nullptr;
        const auto tail = reply->isOpen() ? reply->readAll() : QByteArray{};
        m_received += tail.size();
        if (reply->error() != QNetworkReply::NoError || reply->url().scheme() != "https" ||
            !m_file || m_received != m_total || m_file->write(tail) != tail.size() ||
            !m_file->commit()) {
            fail("The download did not finish. You can try again.");
            return;
        }
        m_file.reset();
        QFile::setPermissions(m_download, QFile::ReadOwner | QFile::WriteOwner);
        setState("Verifying", "Checking the downloaded file…");
        verify("verify");
    });
    m_deadline.start(15 * 60 * 1000);
}
void UpdateService::cancelDownload() {
    if (m_state != "Downloading")
        return;
    ++m_generation;
    m_deadline.stop();
    if (m_reply) {
        m_reply->abort();
        m_reply = nullptr;
    }
    if (m_file) {
        m_file->cancelWriting();
        m_file.reset();
    }
    setState("Available", "Download cancelled. You can download it again later.");
}
void UpdateService::openDownloadFolder() {
    QDir().mkpath(m_folder);
    QDesktopServices::openUrl(QUrl::fromLocalFile(m_folder));
}
bool UpdateService::canInstall() const {
#ifdef Q_OS_LINUX
    return (m_state == "Ready" || m_state == "Preparing") && !busy() &&
           QFileInfo(m_download).isFile() && QFileInfo("/usr/bin/zoru-update").isExecutable() &&
           !QStandardPaths::findExecutable("pkexec").isEmpty();
#elif defined(Q_OS_WIN)
    return (m_state == "Ready" || m_state == "Preparing") && !busy() &&
           QFileInfo(m_download).isFile() && QFileInfo(m_helper).isExecutable();
#else
    return false;
#endif
}
bool UpdateService::prepareInstall() {
    if (!canInstall())
        return false;
    setState("Preparing", "Stopping this mining session before installation…");
    return true;
}
void UpdateService::cancelPreparedInstall() {
    if (m_state == "Preparing")
        fail("The update could not start. Check the download and try again.");
}
void UpdateService::install() {
    if (!canInstall()) {
        cancelPreparedInstall();
        return;
    }
    m_operation = "install";
    m_output.clear();
    setState("Installing", "Installing the update. Approve the system request if asked.");
#ifdef Q_OS_WIN
    const QString runner = QDir(m_storage).filePath(
        "update-runner-" + QUuid::createUuid().toString(QUuid::WithoutBraces) + ".exe");
    if (!QFile::copy(m_helper, runner)) {
        setState("Ready", "The update could not start. Try again.");
        return;
    }
    const QStringList arguments = {"install",
                                   "--metadata",
                                   QDir(m_storage).filePath("metadata.json"),
                                   "--package",
                                   m_download,
                                   "--parent",
                                   QString::number(QCoreApplication::applicationPid()),
                                   "--application",
                                   QCoreApplication::applicationFilePath()};
    if (QProcess::startDetached(runner, arguments))
        emit restartReady();
    else {
        QFile::remove(runner);
        setState("Ready", "The Windows installer could not start. Try again.");
    }
#else
    m_process.setProgram(QStandardPaths::findExecutable("pkexec"));
    m_process.setArguments({"/usr/bin/zoru-update", "install", "--metadata",
                            QDir(m_storage).filePath("metadata.json"), "--package", m_download});
    m_process.start();
    emit changed();
#endif
}
