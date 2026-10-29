#include "WalletService.h"
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>

namespace {
void erase(QByteArray &bytes) {
    bytes.detach();
    volatile char *data = bytes.data();
    for (qsizetype i = 0; i < bytes.size(); ++i)
        data[i] = 0;
    bytes.clear();
}
QJsonObject response(const QByteArray &bytes) {
    const auto result = QJsonDocument::fromJson(bytes).object();
    if (result.contains("ok"))
        return result;
    return {{"ok", false}, {"error", "The wallet action did not finish. Please try again."}};
}
} // namespace
WalletService::WalletService(QObject *parent) : QObject(parent) {
    m_timeout.setSingleShot(true);
    connect(&m_timeout, &QTimer::timeout, &m_process, &QProcess::kill);
    connect(&m_process, &QProcess::started, this, [this] {
        m_process.write(m_input);
        m_process.closeWriteChannel();
        erase(m_input);
    });
    connect(&m_process, &QProcess::readyReadStandardOutput, this, [this] {
        m_output.append(m_process.readAllStandardOutput());
        if (m_output.size() > 16384)
            m_process.kill();
    });
    connect(&m_process, &QProcess::readyReadStandardError, this,
            [this] { m_process.readAllStandardError(); });
    connect(&m_process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished), this,
            [this](int code, QProcess::ExitStatus status) {
                m_timeout.stop();
                m_output.append(m_process.readAllStandardOutput());
                const auto result =
                    code == 0 && status == QProcess::NormalExit && m_output.size() <= 16384
                        ? response(m_output)
                        : QJsonObject{
                              {"ok", false},
                              {"error", "The wallet action did not finish. Please try again."}};
                erase(m_input);
                erase(m_output);
                emit finished(m_action, result);
            });
    connect(&m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart)
            return;
        m_timeout.stop();
        erase(m_input);
        erase(m_output);
        emit finished(m_action,
                      {{"ok", false},
                       {"error", "The wallet service could not start. Reinstall ZORU Miner."}});
    });
}
WalletService::~WalletService() {
    m_process.disconnect(this);
    if (m_process.state() != QProcess::NotRunning) {
        m_process.kill();
        m_process.waitForFinished(1000);
    }
    erase(m_input);
    erase(m_output);
}
QString WalletService::executable() const {
#ifdef Q_OS_WIN
    return QDir(QCoreApplication::applicationDirPath()).filePath("zoru-miner-cli.exe");
#else
    return QDir(QCoreApplication::applicationDirPath()).filePath("zoru-miner-cli");
#endif
}
bool WalletService::ready() const {
    return QFileInfo(executable()).isExecutable();
}
QJsonObject WalletService::status(const QString &directory) const {
    QProcess process;
    process.start(executable(), {"wallet-manage", "--directory", directory});
    if (!process.waitForStarted(3000))
        return {{"ok", false}};
    process.write("{\"action\":\"status\"}");
    process.closeWriteChannel();
    if (!process.waitForFinished(5000)) {
        process.kill();
        process.waitForFinished(1000);
        return {{"ok", false}};
    }
    return response(process.readAllStandardOutput());
}
void WalletService::run(const QString &action, const QString &directory, const QString &file,
                        QByteArray password, QByteArray extra, QByteArray next) {
    if (m_process.state() != QProcess::NotRunning) {
        erase(password);
        erase(extra);
        erase(next);
        return;
    }
    m_action = action;
    QJsonObject input{{"action", action},
                      {"file", file},
                      {"password", QString::fromUtf8(password)},
                      {"extra", QString::fromUtf8(extra)},
                      {"next", QString::fromUtf8(next)}};
    m_input = QJsonDocument(input).toJson(QJsonDocument::Compact);
    input = {};
    erase(password);
    erase(extra);
    erase(next);
    erase(m_output);
    m_process.setProgram(executable());
    m_process.setArguments({"wallet-manage", "--directory", directory});
    m_process.start();
    m_timeout.start(60000);
}
