#include "AppState.h"
#include <QClipboard>
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QMap>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSet>
#include <QStandardPaths>
#include <QSysInfo>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QThread>
#include <QUrl>
#include <QUuid>
#include <algorithm>
#include <cmath>
#ifdef Q_OS_UNIX
#include <signal.h>
#endif
#ifdef Q_OS_MACOS
#include <sys/sysctl.h>
#include <sys/types.h>
#endif
namespace {
void erase(QByteArray &bytes) {
    bytes.detach();
    volatile char *p = bytes.data();
    for (qsizetype i = 0; i < bytes.size(); ++i)
        p[i] = 0;
    bytes.clear();
}
QString localPath(const QString &path) {
    const QUrl url(path);
    return url.isLocalFile() ? url.toLocalFile() : path;
}
QString valueText(const QJsonValue &value) {
    return value.isString() ? value.toString() : QStringLiteral("—");
}
QString amount(const QJsonValue &value, int decimals) {
    if (!value.isString())
        return QStringLiteral("—");
    QString digits = value.toString();
    static const QRegularExpression valid("^[0-9]{1,78}$");
    if (!valid.match(digits).hasMatch())
        return QStringLiteral("—");
    while (digits.size() > 1 && digits.startsWith('0'))
        digits.remove(0, 1);
    digits = digits.rightJustified(decimals + 1, '0');
    QString whole = digits.left(digits.size() - decimals), fraction = digits.right(decimals);
    while (fraction.endsWith('0'))
        fraction.chop(1);
    return fraction.isEmpty() ? whole : whole + '.' + fraction;
}
QString vexFromWei(const QJsonValue &value) {
    return amount(value, 18);
}
QString tokens(const QJsonValue &value) {
    return amount(value, 8);
}
QString feeBudgetWei(const QString &value) {
    static const QRegularExpression valid("^(?:0|[1-9][0-9]{0,59})(?:\\.[0-9]{1,18})?$");
    if (!valid.match(value).hasMatch())
        return {};
    const auto parts = value.split('.');
    QString digits = parts[0] + (parts.size() > 1 ? parts[1] : QString()).leftJustified(18, '0');
    while (digits.size() > 1 && digits.startsWith('0'))
        digits.remove(0, 1);
    static const QString maximum =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    if (digits == "0" || digits.size() > maximum.size() ||
        (digits.size() == maximum.size() && digits > maximum))
        return {};
    return digits;
}
QVariantList bounded(QVariantList values, int limit) {
    while (values.size() > limit)
        values.removeFirst();
    return values;
}
bool isAddress(const QString &value) {
    static const QRegularExpression re("^0x[0-9a-fA-F]{40}$");
    return re.match(value).hasMatch();
}
bool isHash(const QString &value) {
    static const QRegularExpression re("^0x[0-9a-fA-F]{64}$");
    return re.match(value).hasMatch();
}
QString normalizedRpc(const QString &value) {
    const QString text = value.trimmed();
    if (text.size() > 2048)
        return {};
    const QUrl url(text, QUrl::StrictMode);
    const QString host = url.host().toLower();
    if (!url.isValid() || host.isEmpty() || !url.userInfo().isEmpty() || url.hasQuery() ||
        url.hasFragment())
        return {};
    if (url.scheme() != "https" &&
        !(url.scheme() == "http" && QStringList{"localhost", "127.0.0.1", "::1"}.contains(host)))
        return {};
    return url.adjusted(QUrl::NormalizePathSegments).toString(QUrl::FullyEncoded);
}
QString cpuModel() {
#ifdef Q_OS_LINUX
    QFile info("/proc/cpuinfo");
    if (info.open(QIODevice::ReadOnly)) {
        for (const QString &line : QString::fromUtf8(info.read(65536)).split('\n')) {
            if (line.startsWith("model name") || line.startsWith("Hardware"))
                return line.section(':', 1).trimmed();
        }
    }
#elif defined(Q_OS_WIN)
    const QSettings cpu("HKEY_LOCAL_MACHINE\\HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
                        QSettings::NativeFormat);
    const QString name = cpu.value("ProcessorNameString").toString().trimmed();
    if (!name.isEmpty())
        return name;
#elif defined(Q_OS_MACOS)
    size_t size = 0;
    if (sysctlbyname("machdep.cpu.brand_string", nullptr, &size, nullptr, 0) == 0 && size > 0 &&
        size < 1024) {
        QByteArray value(static_cast<int>(size), 0);
        if (sysctlbyname("machdep.cpu.brand_string", value.data(), &size, nullptr, 0) == 0)
            return QString::fromUtf8(value.constData()).trimmed();
    }
#endif
    return QSysInfo::currentCpuArchitecture();
}
QString rateUnit(double rate) {
    return rate >= 1e6 ? "MH/s" : rate >= 1e3 ? "kH/s" : "H/s";
}
QString rateNumber(double rate) {
    const double scale = rate >= 1e6 ? 1e6 : rate >= 1e3 ? 1e3 : 1;
    return QString::number(rate / scale, 'f', scale == 1 ? 0 : 2);
}
} // namespace

AppState::AppState(const QString &storageRoot, bool enableNetwork, QObject *parent)
    : QObject(parent), m_wallet(this), m_updates(this), m_enableNetwork(enableNetwork) {
    m_storage = storageRoot.isEmpty()
                    ? QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation)
                    : storageRoot;
    QDir().mkpath(m_storage);
    QFile::setPermissions(m_storage, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
    m_preferences = std::make_unique<QSettings>(QDir(m_storage).filePath("preferences.ini"),
                                                QSettings::IniFormat);
    m_preferences->setAtomicSyncRequired(true);
    m_updates.initialize(m_storage, enableNetwork);
    connect(&m_updates, &UpdateService::available, this, [this](const QString &version) {
        announce("Version " + version + " is available in Software Updates.");
    });
    connect(&m_updates, &UpdateService::restartReady, this, [] { QCoreApplication::quit(); });
    connect(&m_updates, &UpdateService::changed, this, [this] {
        if (m_updateNetworkSuspended && !m_updates.installing()) {
            m_updateNetworkSuspended = false;
            m_enableNetwork = true;
            startMonitor();
        }
    });
    m_introductionComplete = m_preferences->value("introduction/complete", false).toBool();
    m_cpuModel = ::cpuModel();
    const int cores = qMax(1, QThread::idealThreadCount());
    m_maxThreads = cores >= 4 ? 4 : cores >= 2 ? 2 : 1;
    const int threads = m_preferences->value("mining/threads", suggestedThreads()).toInt();
    m_threads = threads >= 4 && m_maxThreads == 4 ? 4 : threads >= 2 && m_maxThreads >= 2 ? 2 : 1;
    m_intensity = qBound(10, m_preferences->value("mining/intensity", 75).toInt(), 100);
    m_stopAfterProof = m_preferences->value("mining/stopAfterProof", false).toBool();
    m_autoLock = qBound(1, m_preferences->value("security/autoLockMinutes", 15).toInt(), 30);
    m_notifications = m_preferences->value("notifications/enabled", true).toBool();
    for (const auto &candidate : m_preferences->value("connection/rpcUrls").toStringList()) {
        const auto url = normalizedRpc(candidate);
        if (!url.isEmpty() && !m_rpcUrls.contains(url) && m_rpcUrls.size() < 16)
            m_rpcUrls.append(url);
    }
    if (m_rpcUrls.isEmpty())
        m_rpcUrls = {"https://vexascan.com/rpc"};
    const auto selected = normalizedRpc(m_preferences->value("connection/rpcUrl").toString());
    m_rpcUrl = m_rpcUrls.contains(selected) ? selected : m_rpcUrls.first();
    saveRpcPreferences();
    m_sessionBudget = m_preferences->value("mining/sessionBudget", "0.2").toString();
    if (feeBudgetWei(m_sessionBudget).isEmpty())
        m_sessionBudget = "0.2";
    setPage(m_preferences->value("navigation/page", "Overview").toString());
    m_engineAvailable =
        QFileInfo(enginePath()).isExecutable() && QFileInfo(manifestPath()).isFile();
    QFile manifest(manifestPath());
    if (manifest.open(QIODevice::ReadOnly)) {
        const auto config = QJsonDocument::fromJson(manifest.readAll()).object();
        m_deploymentScope = config.value("ledger").toString().toLower();
        for (const auto &item : QList<QPair<QString, QString>>{{"ZORU token", "token"},
                                                               {"Mining Ledger", "ledger"},
                                                               {"PoW Engine", "engine"},
                                                               {"Drand Beacon", "beacon"},
                                                               {"Admin", "admin"}})
            m_contracts.append(QVariantMap{{"name", item.first},
                                           {"address", config.value(item.second).toString()}});
    }
    connect(this, &AppState::walletChanged, this, &AppState::readinessChanged);
    connect(this, &AppState::networkChanged, this, &AppState::readinessChanged);
    connect(this, &AppState::preferencesChanged, this, &AppState::readinessChanged);
    connect(this, &AppState::busyChanged, this, &AppState::readinessChanged);
    connect(this, &AppState::miningChanged, this, &AppState::readinessChanged);
    connect(&m_wallet, &WalletService::finished, this, &AppState::completed);
    m_monitor.setProcessChannelMode(QProcess::SeparateChannels);
    m_miner.setProcessChannelMode(QProcess::SeparateChannels);
    connect(&m_monitor, &QProcess::readyReadStandardOutput, this,
            [this] { consume(m_monitor, m_monitorBuffer, false); });
    connect(&m_miner, &QProcess::readyReadStandardOutput, this,
            [this] { consume(m_miner, m_minerBuffer, true); });
    for (auto *process : {&m_monitor, &m_miner}) {
        connect(process, &QProcess::readyReadStandardError, this, [this, process] {
            const auto bytes = process->readAllStandardError();
            const auto path =
                QDir(m_storage).filePath(process == &m_miner ? "mining.log" : "connection.log");
            if (QFileInfo(path).size() > 262144) {
                QFile::remove(path + ".1");
                QFile::rename(path, path + ".1");
            }
            QFile file(path);
            if (file.open(QIODevice::WriteOnly | QIODevice::Append)) {
                file.setPermissions(QFile::ReadOwner | QFile::WriteOwner);
                file.write(bytes.left(32768));
            }
        });
    }
    connect(&m_monitor, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished), this,
            [this](int code, QProcess::ExitStatus status) {
                consume(m_monitor, m_monitorBuffer, false);
                if (m_closing || !m_enableNetwork)
                    return;
                if (m_monitorRestartPending) {
                    m_monitorRestartPending = false;
                    m_monitorRestart.start(0);
                    return;
                }
                if (code == 0 && status == QProcess::NormalExit && m_healthy) {
                    m_monitorFailures = 0;
                    m_monitorRestart.start(15000);
                } else {
                    networkLost("Reconnecting");
                    m_monitorFailures = qMin(m_monitorFailures + 1, 5);
                    m_monitorRestart.start(qMin(30000, 1000 * (1 << m_monitorFailures)));
                }
            });
    connect(&m_monitor, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (!m_closing && error == QProcess::FailedToStart) {
            networkLost("Backend unavailable");
            m_monitorRestart.start(5000);
        }
    });
    connect(&m_miner, &QProcess::started, this, [this] {
        const auto authorization = m_authorizationEpoch;
        QTimer::singleShot(0, this, [this, authorization] {
            if (authorization != m_authorizationEpoch || m_stopRequested || !m_sessionAuthorized ||
                !m_unlocked || m_startCredential.isEmpty()) {
                erase(m_startCredential);
                m_miner.kill();
                return;
            }
            QJsonObject command{{"action", "connect"},
                                {"password", QString::fromUtf8(m_startCredential)}};
            QByteArray bytes = QJsonDocument(command).toJson(QJsonDocument::Compact) + '\n';
            m_miner.write(bytes);
            command = {};
            erase(bytes);
            erase(m_startCredential);
        });
    });
    connect(
        &m_miner, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished), this,
        [this](int code, QProcess::ExitStatus) {
            consume(m_miner, m_minerBuffer, true);
            if (code != 0 && !m_stopRequested && m_lastMiningIssue.isEmpty())
                m_lastMiningIssue =
                    "The session ended unexpectedly. Check the connection and transaction history.";
            finishSession();
            if (m_updateInstallRequested) {
                beginUpdateInstall();
                return;
            }
            if (!m_closing)
                startMonitor();
        });
    connect(&m_miner, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            m_lastMiningIssue = "The mining backend could not start.";
            finishSession();
            announce(m_lastMiningIssue, true);
        }
    });
    m_monitorRestart.setSingleShot(true);
    connect(&m_monitorRestart, &QTimer::timeout, this, &AppState::startMonitor);
    loadWallet();
    m_lastInteraction = QDateTime::currentMSecsSinceEpoch();
    m_clock.setInterval(1000);
    connect(&m_clock, &QTimer::timeout, this, &AppState::tick);
    m_clock.start();
    QCoreApplication::instance()->installEventFilter(this);
    if (m_enableNetwork)
        QTimer::singleShot(0, this, &AppState::startMonitor);
}
AppState::~AppState() {
    m_closing = true;
    ++m_authorizationEpoch;
    m_sessionAuthorized = false;
    erase(m_pendingCredential);
    erase(m_startCredential);
    QCoreApplication::instance()->removeEventFilter(this);
    m_clock.stop();
    m_monitorRestart.stop();
    stopProcess(m_miner);
    stopProcess(m_monitor);
    m_preferences->sync();
}
QString AppState::enginePath() const {
#ifdef Q_OS_WIN
    return QDir(QCoreApplication::applicationDirPath()).filePath("zoru-miner-cli.exe");
#else
    return QDir(QCoreApplication::applicationDirPath()).filePath("zoru-miner-cli");
#endif
}
QString AppState::manifestPath() const {
    return QDir(QCoreApplication::applicationDirPath())
        .filePath("../share/zoru-miner/deployment.json");
}
QString AppState::statePath() const {
    return QDir(m_storage).filePath("chain-state");
}
QStringList AppState::connectionArguments() const {
    return {"--manifest",  manifestPath(), "--read",   m_rpcUrl,
            "--submit",    m_rpcUrl,       "--native", "https://vexascan.com:8443",
            "--state-dir", statePath()};
}
void AppState::loadWallet() {
    m_hasWallet = QFileInfo(QDir(m_storage).filePath("mining-wallet.json")).isFile();
    const auto status = m_wallet.status(m_storage);
    m_damaged = m_hasWallet && !status.value("ok").toBool();
    m_address = status.value("data").toObject().value("address").toString();
    if (!isAddress(m_address))
        m_address.clear();
    m_backupConfirmed =
        m_preferences->value("wallet/backup/" + m_address.toLower(), false).toBool();
    emit walletChanged();
}
void AppState::setPage(const QString &value) {
    const QStringList pages = {"Overview", "Miner",    "Rewards",  "Wallet",      "Proofs",
                               "Nodes",    "Security", "Settings", "ProofDetail", "Help"};
    if (!pages.contains(value))
        return;
    if (m_page == value)
        return;
    m_page = value;
    if (value != "ProofDetail" && m_preferences)
        m_preferences->setValue("navigation/page", value);
    emit navigationChanged();
}

void AppState::completeIntroduction() {
    m_introductionComplete = true;
    m_preferences->setValue("introduction/complete", true);
    setPage("Wallet");
    emit navigationChanged();
}

void AppState::setIntensity(int value) {
    if (m_running || m_busy) {
        announce("Finish the current session before changing CPU settings.", true);
        return;
    }
    value = qBound(10, value, 100);
    if (value == m_intensity)
        return;
    m_intensity = value;
    m_preferences->setValue("mining/intensity", value);
    emit preferencesChanged();
}

void AppState::setAutoLockMinutes(int value) {
    value = qBound(1, value, 30);
    if (value == m_autoLock)
        return;
    m_autoLock = value;
    m_preferences->setValue("security/autoLockMinutes", value);
    emit preferencesChanged();
}

void AppState::setNotificationsEnabled(bool value) {
    if (value == m_notifications)
        return;
    m_notifications = value;
    m_preferences->setValue("notifications/enabled", value);
    emit preferencesChanged();
}

void AppState::setRpcUrl(const QString &value) {
    if (m_running || m_busy) {
        announce("Stop mining and finish the wallet action before switching RPC.", true);
        return;
    }
    const QString url = normalizedRpc(value);
    if (url.isEmpty() || !m_rpcUrls.contains(url)) {
        announce("Add a valid RPC in Nodes before selecting it.", true);
        return;
    }
    if (m_rpcUrl == url)
        return;
    m_rpcUrl = url;
    m_activeRpc.clear();
    m_nodes.clear();
    m_latencySamples.clear();
    m_latency = "—";
    m_height = "—";
    saveRpcPreferences();
    networkLost("Connecting");
    emit preferencesChanged();
    startMonitor();
}

void AppState::saveRpcPreferences() {
    m_preferences->setValue("connection/rpcUrls", m_rpcUrls);
    m_preferences->setValue("connection/rpcUrl", m_rpcUrl);
}

bool AppState::addRpc(const QString &value) {
    if (m_running || m_busy) {
        announce("Stop mining and finish the wallet action before editing RPC connections.", true);
        return false;
    }
    const QString url = normalizedRpc(value);
    if (url.isEmpty()) {
        announce("Enter an HTTPS RPC URL without credentials, a query or a fragment. Local RPC may "
                 "use HTTP.",
                 true);
        return false;
    }
    if (m_rpcUrls.contains(url)) {
        announce("This RPC is already in your list.", true);
        return false;
    }
    if (m_rpcUrls.size() >= 16) {
        announce("Remove an unused RPC before adding another.", true);
        return false;
    }
    m_rpcUrls.append(url);
    saveRpcPreferences();
    emit networkChanged();
    emit preferencesChanged();
    startMonitor();
    return true;
}

bool AppState::removeRpc(const QString &value) {
    if (m_running || m_busy) {
        announce("Stop mining and finish the wallet action before editing RPC connections.", true);
        return false;
    }
    const QString url = normalizedRpc(value);
    if (!m_rpcUrls.contains(url))
        return false;
    if (m_rpcUrls.size() == 1) {
        announce("Keep at least one RPC connection.", true);
        return false;
    }
    m_rpcUrls.removeAll(url);
    if (m_rpcUrl == url) {
        m_rpcUrl = m_rpcUrls.first();
        m_latencySamples.clear();
        m_nodes.clear();
        m_latency = "—";
        m_height = "—";
        networkLost("Connecting");
    }
    saveRpcPreferences();
    emit networkChanged();
    emit preferencesChanged();
    startMonitor();
    return true;
}

QVariantList AppState::nodes() const {
    QVariantList result;
    for (const QString &url : m_rpcUrls) {
        QVariantMap row{{"url", url}, {"status", "Not checked"}, {"healthy", false}};
        for (const auto &item : m_nodes)
            if (item.toMap().value("url").toString() == url) {
                row = item.toMap();
                break;
            }
        const bool selected = url == m_rpcUrl;
        row["selected"] = selected;
        if (selected) {
            if (m_healthy) {
                row["status"] = "Synced";
                row["healthy"] = true;
            } else {
                row["healthy"] = false;
                if (m_network == "Wrong network" || m_network == "Network mismatch" ||
                    m_network == "Unsynced")
                    row["status"] = m_network;
                else if (row.value("status") == "Connected")
                    row["status"] = "Verifying";
            }
        }
        result.append(row);
    }
    return result;
}

QString AppState::budgetWei() const {
    return feeBudgetWei(m_sessionBudget);
}

QString AppState::approvedSessionBudget() const {
    if (m_sessionFeeLimit != "—")
        return m_sessionFeeLimit;
    return m_approvedBudget.isEmpty() ? m_sessionBudget : vexFromWei(QJsonValue(m_approvedBudget));
}

void AppState::setSessionBudget(const QString &value) {
    if (m_running || m_busy || feeBudgetWei(value).isEmpty()) {
        announce(
            "Choose a positive session fee limit with at most 18 decimal places before starting.",
            true);
        return;
    }
    m_sessionBudget = value;
    m_preferences->setValue("mining/sessionBudget", value);
    emit preferencesChanged();
}

void AppState::announce(const QString &message, bool error) {
    if (m_lastNotice == message && m_lastNoticeError == error && m_noticeClock.isValid() &&
        m_noticeClock.elapsed() < 5000)
        return;
    m_lastNotice = message;
    m_lastNoticeError = error;
    m_noticeClock.restart();
    if (m_notifications || error)
        emit notification(message, error);
}

void AppState::walletAction(const QString &action, const QString &password,
                            const QString &confirmation, const QString &file,
                            const QString &oldPassword, const QString &recoverySecret,
                            const QString &method) {
    if (m_busy)
        return;
    const QStringList valid = {"create",      "unlock",    "import", "recovery",
                               "backup",      "password",  "remove", "archive",
                               "acknowledge", "authorize", "reveal"};
    if (!valid.contains(action))
        return;
    if (!m_wallet.ready()) {
        announce("The wallet is unavailable. Reinstall ZORU Miner.", true);
        return;
    }
    if ((action == "create" || action == "import" || action == "recovery") && m_hasWallet) {
        announce("A wallet is already stored here. Use wallet recovery to replace it.", true);
        return;
    }
    if (m_running && (action == "remove" || action == "archive" || action == "password")) {
        announce("Stop mining before changing the wallet.", true);
        return;
    }
    if ((action == "create" || action == "import" || action == "recovery" ||
         action == "password") &&
        (password.toUcs4().size() < 12 || password.toUtf8().size() > 1024 ||
         password.contains('\n') || password.contains('\r') || password.contains(QChar::Null))) {
        announce("Use a password with at least 12 characters and no line breaks.", true);
        return;
    }
    if (action != "archive" && password.isEmpty()) {
        announce("Enter your wallet password.", true);
        return;
    }
    m_walletAuthorizationEpoch = m_authorizationEpoch;
    m_walletLockEpoch = m_lockEpoch;
    m_busy = true;
    emit busyChanged();
    if (action == "import")
        m_wallet.run(action, m_storage, localPath(file), oldPassword.toUtf8(), {},
                     password.toUtf8());
    else if (action == "recovery") {
        if (QGuiApplication::clipboard()->text().trimmed() == recoverySecret.trimmed())
            QGuiApplication::clipboard()->clear();
        m_wallet.run(action, m_storage, method, password.toUtf8(), recoverySecret.toUtf8());
    } else if (action == "password")
        m_wallet.run(action, m_storage, {}, oldPassword.toUtf8(), {}, password.toUtf8());
    else
        m_wallet.run(action, m_storage, localPath(file), password.toUtf8(), confirmation.toUtf8());
}

void AppState::completed(const QString &action, const QJsonObject &result) {
    m_busy = false;
    emit busyChanged();
    if (m_closing) {
        erase(m_pendingCredential);
        return;
    }
    if (m_walletLockEpoch != m_lockEpoch ||
        (action == "authorize" && m_walletAuthorizationEpoch != m_authorizationEpoch)) {
        erase(m_pendingCredential);
        m_signAfterUnlock = false;
        loadWallet();
        m_unlocked = false;
        emit walletChanged();
        emit walletActionFinished(action, false,
                                  "Wallet access remains locked. Start again when you are ready.");
        return;
    }
    if (!result.value("ok").toBool()) {
        erase(m_pendingCredential);
        m_signAfterUnlock = false;
        const QString message =
            result.value("error").toString("This wallet action could not be completed.");
        announce(message, true);
        emit walletActionFinished(action, false, message);
        return;
    }
    const QJsonObject data = result.value("data").toObject();
    QString message;
    if (action == "reveal") {
        message = "Keep your recovery details private.";
    } else if (action == "remove" || action == "archive") {
        clearWalletProjection();
        loadWallet();
        startMonitor();
        message = action == "archive"
                      ? "Encrypted wallet preserved. Import your recovery details to continue."
                      : "Local wallet removed. Your external backups are unchanged.";
    } else if (action == "create" || action == "import" || action == "recovery" ||
               action == "unlock" || action == "authorize" || action == "acknowledge") {
        // Never reuse verification for a different wallet if the keystore was
        // replaced while the mining password was being checked.
        if (action == "authorize" &&
            (!isAddress(data.value("address").toString()) ||
             data.value("address").toString().compare(m_address, Qt::CaseInsensitive) != 0)) {
            erase(m_pendingCredential);
            m_signAfterUnlock = false;
            const QString error =
                "Wallet changed during authorization. Reopen ZORU Miner before mining.";
            announce(error, true);
            emit walletActionFinished(action, false, error);
            return;
        }
        const bool addressChanged =
            m_address.compare(data.value("address").toString(), Qt::CaseInsensitive) != 0;
        m_hasWallet = true;
        m_unlocked = true;
        m_damaged = false;
        m_address = data.value("address").toString();
        m_lastInteraction = QDateTime::currentMSecsSinceEpoch();
        if (action == "import")
            m_backupConfirmed = !data.value("backupRequired").toBool(true);
        else if (action == "acknowledge")
            m_backupConfirmed = true;
        else if (action == "create" || action == "recovery")
            m_backupConfirmed = false;
        else
            m_backupConfirmed =
                m_preferences->value("wallet/backup/" + m_address.toLower(), false).toBool();
        m_preferences->setValue("wallet/backup/" + m_address.toLower(), m_backupConfirmed);
        m_introductionComplete = true;
        m_preferences->setValue("introduction/complete", true);
        if (action == "create" || action == "import" || action == "recovery")
            setPage("Wallet");
        // Wallet access does not change the observer's identity. Preserve its
        // verified connection unless the account being observed actually changed.
        if (addressChanged)
            startMonitor();
        message = action == "create"   ? "Wallet created. Save an encrypted backup before mining."
                  : action == "unlock" ? "Wallet unlocked."
                  : action == "acknowledge" ? "Recovery backup confirmed."
                                            : "Wallet ready.";
        if (action == "authorize" && m_signAfterUnlock) {
            m_signAfterUnlock = false;
            launchMiner(std::move(m_pendingCredential));
            if (!m_running) {
                emit walletActionFinished(
                    action, false, "The connection changed. Confirm mining again when ready.");
                return;
            }
        }
    } else if (action == "backup") {
        m_backupConfirmed = true;
        m_preferences->setValue("wallet/backup/" + m_address.toLower(), true);
        message = "Encrypted backup saved. Keep it separate from this device.";
    } else if (action == "password") {
        m_unlocked = false;
        m_backupConfirmed = false;
        m_preferences->setValue("wallet/backup/" + m_address.toLower(), false);
        message = "Password updated. Unlock your wallet with the new password.";
    }
    emit walletChanged();
    emit navigationChanged();
    if (action != "authorize")
        announce(message);
    emit walletActionFinished(action, true, message);
    if (action == "reveal")
        emit recoveryDetails(data.value("kind").toString(), data.value("secret").toString());
    else if (action == "create")
        emit recoveryDetails(data.value("recoveryKind").toString(),
                             data.value("recoverySecret").toString());
}

void AppState::lockWallet() {
    ++m_lockEpoch;
    stopMining();
    erase(m_pendingCredential);
    m_signAfterUnlock = false;
    m_unlocked = false;
    emit walletChanged();
    announce("Wallet locked. Mining is stopped.");
}

void AppState::setCpuThreads(int value) {
    if (m_running || m_busy) {
        announce("Finish the current session before changing CPU settings.", true);
        return;
    }
    if (!QList<int>{1, 2, 4}.contains(value) || value > m_maxThreads) {
        announce("Choose 1, 2 or 4 threads within your CPU capacity.", true);
        return;
    }
    m_threads = value;
    m_preferences->setValue("mining/threads", value);
    emit preferencesChanged();
}
void AppState::setStopAfterProof(bool value) {
    if (m_running || m_busy)
        return;
    m_stopAfterProof = value;
    m_preferences->setValue("mining/stopAfterProof", value);
    emit preferencesChanged();
}
void AppState::clearWalletProjection() {
    m_address.clear();
    m_hasWallet = false;
    m_unlocked = false;
    m_backupConfirmed = false;
    m_damaged = false;
    m_nativeBalance = m_tokenBalance = m_confirmedRewards = m_myPending = m_myUnclaimed = "—";
    m_confirmedProofs = 0;
    m_proofs.clear();
    m_rewardSamples.clear();
    m_historyComplete = false;
    m_historyStatus = "Set up a wallet to view your activity";
    m_selectedId.clear();
    m_selectedDigest.clear();
    emit walletChanged();
    emit historyChanged();
    emit networkChanged();
}
bool AppState::pendingTransaction() const {
    QString root = qEnvironmentVariable("ZORU_STATE_ROOT");
    if (root.isEmpty())
        root = QDir::home().filePath(".zoru");
    const QDir journal(
        QDir(root).filePath("signers/mainnet/" + m_address.toLower() + "/private-journal"));
    for (const auto &name : journal.entryList({"*.json"}, QDir::Files)) {
        QFile file(journal.filePath(name));
        if (file.size() > 1024 * 1024 || !file.open(QIODevice::ReadOnly))
            continue;
        const auto state =
            QJsonDocument::fromJson(file.readAll()).object().value("state").toString();
        if (state != "confirmed" && state != "superseded")
            return true;
    }
    return false;
}
QStringList AppState::miningHolds() const {
    if (!m_running)
        return {};
    QSet<QString> holds = m_engineHolds;
    if (m_manualPauseRequested)
        holds.insert("manual");
    if (m_securityHeld)
        holds.insert("security");
    auto result = holds.values();
    result.sort();
    return result;
}
bool AppState::activelyHashing() const {
    return m_running && m_sessionAuthorized && !m_stopRequested && m_hashing &&
           miningHolds().isEmpty();
}
QString AppState::miningExplanation() const {
    const auto holds = miningHolds();
    if (holds.contains("security"))
        return "Mining stopped because the verified network identity changed.";
    if (holds.contains("manual"))
        return "Paused by you. Transactions already sent can still confirm.";
    if (holds.contains("resource"))
        return "Hashing is paused while CPU load or available memory is outside the miner's "
               "limits.";
    if (!m_running)
        return "No active mining session.";
    if (m_pendingAction == "claim")
        return "Claiming eligible proof rewards. Each claim uses a separate transaction.";
    if (activelyHashing())
        return "CPU hashing is active. Accepted proofs share their epoch's reward budget.";
    return "Waiting for verified work or transaction confirmation. Rewards are claimed separately "
           "from this interface.";
}
QString AppState::networkReadiness() const {
    return m_healthy ? "RPC connected · Chain verified" : m_network;
}
void AppState::networkLost(const QString &state, bool security) {
    m_healthy = false;
    m_network = state;
    if (security && m_running) {
        m_securityHeld = true;
        stopMining();
    }
    emit networkChanged();
    emit miningChanged();
}
QString AppState::miningBlocker() const {
    if (!engineReady())
        return "The mining engine or deployment manifest is missing.";
    if (!m_hasWallet || m_damaged || !isAddress(m_address))
        return "Create or import a valid wallet first.";
    if (!m_backupConfirmed)
        return "Save an encrypted backup or confirm your recovery backup first.";
    if (!m_healthy)
        return "Wait for a verified mainnet connection.";
    if (m_chainMiningPaused)
        return "Proof admission is paused by the contract.";
    if (budgetWei().isEmpty())
        return "Save a positive session fee allowance.";
    return {};
}
bool AppState::startMining(const QString &password, bool authorized) {
    if (m_running || m_busy || m_miner.state() != QProcess::NotRunning)
        return false;
    const auto blocker = miningBlocker();
    if (!blocker.isEmpty()) {
        announce(blocker, true);
        return false;
    }
    m_pendingAction = "mine";
    return authorizeSession(password, authorized);
}
bool AppState::claimRewards(const QString &password, bool authorized) {
    if (m_running || m_busy || !m_hasWallet || m_damaged || !m_healthy)
        return false;
    if (m_claimsPaused) {
        announce("Reward claims are paused by the contract.", true);
        return false;
    }
    if (!m_historyComplete) {
        announce("Wait for your proof history to finish syncing before claiming.", true);
        return false;
    }
    m_pendingAction = "claim";
    return authorizeSession(password, authorized);
}
bool AppState::authorizeSession(const QString &password, bool authorized) {
    if (!authorized) {
        announce("Review and approve the session fee allowance first.", true);
        return false;
    }
    if (password.isEmpty() || password.toUtf8().size() > 1024 || password.contains('\n') ||
        password.contains('\r') || password.contains(QChar::Null)) {
        announce("Enter your wallet password.", true);
        return false;
    }
    ++m_authorizationEpoch;
    m_approvedBudget = budgetWei();
    m_authorizedAddress = m_address;
    m_lastMiningIssue.clear();
    m_pendingCredential = password.toUtf8();
    m_signAfterUnlock = true;
    walletAction("authorize", password);
    return m_busy;
}
void AppState::launchMiner(QByteArray password) {
    const bool claim = m_pendingAction == "claim";
    if (m_closing || !m_healthy || (!claim && !miningBlocker().isEmpty()) ||
        m_authorizedAddress.compare(m_address, Qt::CaseInsensitive) != 0 ||
        m_approvedBudget.isEmpty()) {
        erase(password);
        announce("The wallet or connection changed. Review the session again.", true);
        return;
    }
    if (!claim) {
        m_found = 0;
        m_sessionAccepted = 0;
        m_sessionRejected = 0;
        m_hashrate = 0;
        m_lastHashrate = -1;
        m_lastMeasuredAt = 0;
    }
    m_engineStateKnown = false;
    m_beaconVerified = false;
    m_sessionFeeLimit = m_sessionBudget;
    m_lastTransactionFeeLimit = "—";
    m_sessionFeesSpent = "0";
    m_sessionFeesReserved = "0";
    m_sessionFeesRemaining = m_sessionBudget;
    if (!claim) {
        m_sessionHashes = "0";
        m_hashrateSamples.clear();
        m_started = QDateTime::currentMSecsSinceEpoch();
        m_sessionEnded = 0;
    }
    m_minerBuffer.clear();
    m_lastMiningIssue.clear();
    m_sessionEndStatus.clear();
    m_engineHolds.clear();
    m_manualPauseRequested = false;
    m_securityHeld = false;
    m_hashing = false;
    m_stopRequested = false;
    m_sessionAuthorized = true;
    m_startCredential = std::move(password);
    auto arguments = QStringList{"serve"} + connectionArguments();
    arguments << "--wallet" << QDir(m_storage).filePath("mining-wallet.json") << "--expected-wallet"
              << m_authorizedAddress << "--session-budget-wei" << m_approvedBudget;
    m_miner.setProgram(enginePath());
    m_miner.setArguments(arguments);
    m_running = true;
    m_miningStatus = "Verifying wallet and deployment";
    m_miner.start();
    emit miningChanged();
    emit statsChanged();
}
bool AppState::sendControl(const QString &action) {
    if (m_miner.state() != QProcess::Running)
        return false;
    return m_miner.write(
               QJsonDocument(QJsonObject{{"action", action}}).toJson(QJsonDocument::Compact) +
               "\n") >= 0;
}
void AppState::pauseMining() {
    if (!m_running || !m_sessionAuthorized || m_stopRequested)
        return;
    m_manualPauseRequested = true;
    m_hashing = false;
    m_hashrate = 0;
    sendControl("pause");
    m_miningStatus = "Pausing";
    emit miningChanged();
    emit statsChanged();
}
void AppState::resumeMining() {
    if (!m_running || !m_sessionAuthorized || !m_unlocked || m_stopRequested)
        return;
    m_manualPauseRequested = false;
    sendControl("resume");
    m_miningStatus = "Resuming";
    emit miningChanged();
}
void AppState::stopMining() {
    ++m_authorizationEpoch;
    m_signAfterUnlock = false;
    m_sessionAuthorized = false;
    erase(m_pendingCredential);
    erase(m_startCredential);
    m_hashing = false;
    m_hashrate = 0;
    emit miningChanged();
    emit statsChanged();
    if (m_miner.state() == QProcess::NotRunning || m_stopRequested)
        return;
    m_stopRequested = true;
    m_miningStatus = "Stopping";
    sendControl("quit");
    emit miningChanged();
    const auto pid = m_miner.processId();
    QTimer::singleShot(6500, this, [this, pid] {
        if (m_stopRequested && m_miner.state() != QProcess::NotRunning &&
            m_miner.processId() == pid)
            m_miner.kill();
    });
}
void AppState::finishSession() {
    if (m_pendingAction != "claim")
        m_sessionEnded = QDateTime::currentMSecsSinceEpoch();
    m_sessionAuthorized = false;
    m_running = false;
    m_stopRequested = false;
    m_engineStateKnown = false;
    m_hashing = false;
    m_hashrate = 0;
    m_manualPauseRequested = false;
    m_engineHolds.clear();
    erase(m_pendingCredential);
    erase(m_startCredential);
    m_signAfterUnlock = false;
    ++m_authorizationEpoch;
    m_miningStatus = !m_sessionEndStatus.isEmpty()  ? m_sessionEndStatus
                     : !m_lastMiningIssue.isEmpty() ? "Stopped · check session details"
                                                    : "Stopped";
    emit miningChanged();
    emit statsChanged();
}
void AppState::stopProcess(QProcess &process) {
    if (process.state() == QProcess::NotRunning)
        return;
    if (&process == &m_miner) {
        process.write("{\"action\":\"quit\"}\n");
        process.closeWriteChannel();
    } else
        process.terminate();
    if (!process.waitForFinished(1500)) {
        process.kill();
        process.waitForFinished(1000);
    }
}
void AppState::startMonitor() {
    if (!m_enableNetwork || m_closing || !m_engineAvailable)
        return;
    if (m_monitor.state() != QProcess::NotRunning) {
        m_monitorRestartPending = true;
        return;
    }
    m_monitorRestart.stop();
    m_monitorBuffer.clear();
    m_monitorAddress = m_address;
    m_monitorRpc = m_rpcUrl;
    auto arguments = QStringList{"rewards"} + connectionArguments();
    if (isAddress(m_address))
        arguments << "--account" << m_address;
    m_monitor.setProgram(enginePath());
    m_monitor.setArguments(arguments);
    m_monitor.start();
}
void AppState::consume(QProcess &process, QByteArray &buffer, bool miner) {
    buffer.append(process.readAllStandardOutput());
    consumeFrames(buffer, miner);
}
void AppState::consumeFrames(QByteArray &buffer, bool miner) {
    const auto overflow = [&] {
        buffer.clear();
        if (miner) {
            stopMining();
            announce("Mining stopped because a session update could not be read. Transaction "
                     "journals are preserved.",
                     true);
        } else {
            networkLost("Reconnecting");
            m_monitorRestart.start(2000);
        }
    };
    qsizetype offset = 0, index = 0;
    while ((index = buffer.indexOf('\n', offset)) >= 0) {
        if (index - offset > 1024 * 1024) {
            overflow();
            return;
        }
        const QByteArray line = buffer.mid(offset, index - offset);
        offset = index + 1;
        const QJsonDocument document = QJsonDocument::fromJson(line);
        if (!document.isObject())
            continue;
        if (miner)
            applyMinerEvent(document.object());
        else
            applyMonitorEvent(document.object());
    }
    // Bound individual frames and an unfinished tail, not a valid burst of
    // many small frames. Compact once rather than shifting on every message.
    buffer.remove(0, offset);
    if (buffer.size() > 1024 * 1024)
        overflow();
}

void AppState::applyMonitorEvent(const QJsonObject &event) {
    if (event.value("event") == "error") {
        networkLost("Read-only RPC check failed");
        return;
    }
    if (event.value("event") != "inspection")
        return;
    if ((!m_monitorRpc.isEmpty() && m_monitorRpc != m_rpcUrl) ||
        (!m_monitorAddress.isEmpty() &&
         m_monitorAddress.compare(m_address, Qt::CaseInsensitive) != 0))
        return;
    applyInspection(event.value("data").toObject());
}
void AppState::applyInspection(const QJsonObject &data) {
    if (data.value("chainId").toInt() != 6736 || !isHash(data.value("blockHash").toString())) {
        networkLost("Invalid mainnet snapshot", true);
        return;
    }
    m_healthy = true;
    m_network = "Synced";
    m_monitorFailures = 0;
    m_activeRpc = m_rpcUrl;
    m_lastSnapshot = QDateTime::currentMSecsSinceEpoch();
    m_blockTimestamp = data.value("blockTimestamp").toString().toLongLong();
    m_height = valueText(data.value("blockNumber"));
    m_latency = QString::number(data.value("rpcLatencyMilliseconds").toDouble(), 'f', 0) + " ms";
    m_latencySamples = bounded(
        m_latencySamples +
            QVariantList{QVariantMap{{"timestamp", m_lastSnapshot / 1000},
                                     {"value", data.value("rpcLatencyMilliseconds").toDouble()}}},
        120);
    m_epoch = valueText(data.value("currentEpoch"));
    m_difficulty = valueText(data.value("expectedHashes"));
    m_totalSupply = tokens(data.value("totalSupplyBaseUnits"));
    m_reward = tokens(data.value("nextEmissionBudgetBaseUnits"));
    m_chainMiningPaused =
        data.value("admissionPaused").toBool() || !data.value("miningLaunched").toBool();
    m_claimsPaused = data.value("claimsPaused").toBool();
    m_globalStats = {{"transactionCap", vexFromWei(data.value("transactionFeeCapWei"))},
                     {"allocated", tokens(data.value("allocatedSupplyBaseUnits"))},
                     {"funded", tokens(data.value("fundedSupplyBaseUnits"))},
                     {"paid", tokens(data.value("paidSupplyBaseUnits"))},
                     {"pending", tokens(data.value("globalPendingBaseUnits"))},
                     {"unclaimed", tokens(data.value("globalUnclaimedBaseUnits"))},
                     {"totalProofs", valueText(data.value("totalAcceptedProofs"))},
                     {"epochProofs", valueText(data.value("currentEpochProofs"))},
                     {"epochBudget", tokens(data.value("currentEpochBudgetBaseUnits"))},
                     {"emissionIndex", valueText(data.value("emissionIndex"))},
                     {"claimsPaused", m_claimsPaused},
                     {"admissionPaused", m_chainMiningPaused},
                     {"blockHash", data.value("blockHash").toString()}};
    m_halving = {{"currentReward", m_reward},
                 {"nextReward", tokens(data.value("followingEraBudgetBaseUnits"))},
                 {"epochsUntilHalving", valueText(data.value("epochsUntilHalving"))},
                 {"halvingEra", valueText(data.value("halvingEra"))},
                 {"maximumSupply", tokens(data.value("capBaseUnits"))},
                 {"lifetimeMinted", tokens(data.value("lifetimeMintedBaseUnits"))},
                 {"halvingProgressEpochs", valueText(data.value("halvingProgressEpochs"))},
                 {"halvingEpochs", valueText(data.value("halvingEpochs"))},
                 {"issuanceEnded", m_reward == "0"}};
    m_nodes = {QVariantMap{
        {"url", m_rpcUrl}, {"status", "Verified"}, {"healthy", true}, {"selected", true}}};
    if (isAddress(m_address) &&
        data.value("address").toString().compare(m_address, Qt::CaseInsensitive) == 0) {
        m_tokenBalance = tokens(data.value("tokenBalanceBaseUnits"));
        m_nativeBalance = vexFromWei(data.value("nativeBalanceWei"));
        m_myPending = tokens(data.value("myPendingBaseUnits"));
        m_myUnclaimed = tokens(data.value("myUnclaimedBaseUnits"));
        m_confirmedRewards = tokens(data.value("myClaimedBaseUnits"));
        m_historyComplete = data.value("myRewardsHistoryComplete").toBool();
        m_confirmedProofs = data.value("myClaimedProofs").toString().toULongLong();
        m_historyStatus = m_historyComplete
                              ? "On-chain proof history synced · latest 100 proofs shown"
                              : "Syncing on-chain proof history";
        QVariantList proofs;
        for (const auto &entry : data.value("proofs").toArray()) {
            const auto proof = entry.toObject();
            const auto digest = proof.value("digest").toString();
            const bool canonical = proof.value("canonical").toBool();
            const bool claimed = canonical && proof.value("claimed").toBool();
            QVariantMap row{{"id", digest},
                            {"digest", digest},
                            {"epoch", proof.value("epoch").toVariant().toString()},
                            {"transactionHash", proof.value("hash").toString()},
                            {"blockNumber", proof.value("block").toVariant().toString()},
                            {"claimBlock", proof.value("claim_block").toVariant().toString()},
                            {"timestamp", proof.value("epochStart").toString().toLongLong()},
                            {"status", !canonical ? "Reorged"
                                       : claimed  ? "Reward paid"
                                                  : "Accepted"},
                            {"reward", claimed ? tokens(proof.value("reward")) : QString("—")}};
            proofs.append(row);
        }
        m_proofs = proofs;
        m_rewardSamples = bounded(
            m_rewardSamples + QVariantList{QVariantMap{{"timestamp", m_blockTimestamp},
                                                       {"value", m_confirmedRewards.toDouble()}}},
            180);
    }
    emit networkChanged();
    emit historyChanged();
}
void AppState::applyFees(const QJsonObject &fees) {
    m_sessionFeeLimit = vexFromWei(fees.value("limitWei"));
    m_sessionFeesSpent = vexFromWei(fees.value("spentWei"));
    m_sessionFeesReserved = vexFromWei(fees.value("reservedWei"));
    m_sessionFeesRemaining = vexFromWei(fees.value("remainingWei"));
    m_lastTransactionFeeLimit = vexFromWei(fees.value("lastMaximumFeeWei"));
}
void AppState::applyMinerEvent(const QJsonObject &event) {
    if (!m_running)
        return;
    const auto type = event.value("event").toString();
    const auto data = event.value("data").toObject();
    if (type == "inspection") {
        applyInspection(data);
        return;
    }
    if (type == "error") {
        if (m_stopRequested)
            return;
        m_lastMiningIssue = event.value("error").toString();
        if (m_lastMiningIssue.startsWith("FEE_BUDGET_EXHAUSTED:"))
            m_sessionEndStatus = "Fee allowance reached";
        announce(m_lastMiningIssue, true);
        emit miningChanged();
        return;
    }
    if (type == "command-complete") {
        const auto action = event.value("action").toString();
        if (action == "connect") {
            if (!m_sessionAuthorized || m_stopRequested || !m_lastMiningIssue.isEmpty()) {
                sendControl("quit");
                return;
            }
            m_engineStateKnown = true;
            if (m_pendingAction == "claim") {
                sendControl("claim");
                m_miningStatus = "Claiming rewards";
            } else {
                const double budget = qMin(2.0, double(m_threads) * m_intensity / 100.0);
                const QJsonObject command{{"action", "mine"},
                                          {"workers", m_threads},
                                          {"cpuBudget", budget},
                                          {"maxAccepted", m_stopAfterProof ? 1 : 0}};
                m_miner.write(QJsonDocument(command).toJson(QJsonDocument::Compact) + "\n");
                if (m_manualPauseRequested)
                    sendControl("pause");
            }
        } else if (action == "mine" || action == "claim") {
            sendControl("quit");
        }
        return;
    }
    if (type == "claims") {
        m_sessionEndStatus =
            QString::number(data.value("claims").toArray().size()) + " rewards claimed";
        return;
    }
    if (type != "state" && type != "mine-result")
        return;
    applyFees(data.value("fees").toObject());
    if (m_pendingAction == "claim") {
        emit statsChanged();
        return;
    }
    m_found = data.value("validFound").toVariant().toULongLong();
    m_sessionAccepted = data.value("accepted").toInt();
    m_sessionRejected = data.value("rejected").toInt();
    m_sessionHashes = data.value("hashes").toVariant().toString();
    const auto phase = data.value("status").toString();
    m_engineHolds.clear();
    if (phase == "paused")
        m_engineHolds.insert("manual");
    if (phase == "paused-resource")
        m_engineHolds.insert("resource");
    m_hashing = phase == "mining";
    const auto contexts = data.value("contexts").toArray();
    if (!contexts.isEmpty()) {
        const auto work = contexts.last().toObject();
        m_roundEpoch = work.value("epoch").toVariant().toString();
        m_beaconVerified = !work.value("seed").toString().isEmpty() && phase == "mining";
    }
    const double rate = data.value("hashesPerSecond").toDouble();
    if (std::isfinite(rate) && rate > 0 && m_hashing) {
        m_hashrate = rate;
        m_lastHashrate = rate;
        m_lastMeasuredAt = QDateTime::currentMSecsSinceEpoch();
        m_hashrateSamples = bounded(
            m_hashrateSamples +
                QVariantList{QVariantMap{{"timestamp", m_lastMeasuredAt / 1000}, {"value", rate}}},
            180);
    } else
        m_hashrate = 0;
    if (m_stopRequested)
        m_miningStatus = "Stopping";
    else if (phase == "mining")
        m_miningStatus = "Mining";
    else if (phase == "paused")
        m_miningStatus = "Paused";
    else if (phase == "paused-resource")
        m_miningStatus = "Waiting for CPU or memory";
    else if (phase == "waiting-beacon-or-recovery")
        m_miningStatus = "Waiting for verified beacon";
    else
        m_miningStatus = phase;
    if (type == "mine-result" && m_sessionEndStatus.isEmpty())
        m_sessionEndStatus =
            m_stopAfterProof && m_sessionAccepted > 0 ? "Completed · proof accepted" : "Stopped";
    emit miningChanged();
    emit statsChanged();
}
int AppState::sessionAccepted() const {
    return m_sessionAccepted;
}
int AppState::sessionRejected() const {
    return m_sessionRejected;
}
QString AppState::efficiency() const {
    const int total = sessionAccepted() + sessionRejected();
    return total ? QString::number(100.0 * sessionAccepted() / total, 'f', 1) + "%"
                 : QStringLiteral("—");
}

QString AppState::hashrateUnit() const {
    return rateUnit(activelyHashing() ? m_hashrate : m_lastHashrate >= 0 ? m_lastHashrate : 0);
}

QString AppState::hashrateValue() const {
    if (!activelyHashing())
        return m_lastHashrate >= 0 ? rateNumber(m_lastHashrate) : QStringLiteral("—");
    if (m_hashrate < 0)
        return "—";
    return rateNumber(m_hashrate);
}

QString AppState::lastMeasuredHashrate() const {
    return m_lastHashrate < 0 ? QStringLiteral("—")
                              : rateNumber(m_lastHashrate) + " " + rateUnit(m_lastHashrate);
}

QString AppState::lastMeasuredAt() const {
    return m_lastMeasuredAt > 0
               ? QDateTime::fromMSecsSinceEpoch(m_lastMeasuredAt).toString("HH:mm:ss")
               : QStringLiteral("—");
}

QString AppState::uptime() const {
    if (m_started <= 0)
        return "—";
    const qint64 elapsed = qMax<qint64>(
        0,
        ((m_sessionEnded > 0 ? m_sessionEnded : QDateTime::currentMSecsSinceEpoch()) - m_started) /
            1000);
    return QString("%1h %2m %3s").arg(elapsed / 3600).arg((elapsed / 60) % 60).arg(elapsed % 60);
}

QString AppState::sessionStarted() const {
    return m_started ? QDateTime::fromMSecsSinceEpoch(m_started).toString("MMM d, HH:mm:ss")
                     : QStringLiteral("—");
}

QString AppState::blockAge() const {
    if (!m_blockTimestamp)
        return "Waiting for a block";
    return QString::number(qMax<qint64>(0, QDateTime::currentSecsSinceEpoch() - m_blockTimestamp)) +
           "s ago";
}

QVariantList AppState::contracts() const {
    QVariantList values;
    for (const auto &entry : m_contracts) {
        auto row = entry.toMap();
        row["verified"] = m_healthy;
        values.append(row);
    }
    return values;
}

QVariantMap AppState::selectedProof() const {
    for (const auto &entry : m_proofs)
        if (entry.toMap().value("id") == m_selectedId ||
            (!m_selectedDigest.isEmpty() && entry.toMap().value("digest") == m_selectedDigest))
            return entry.toMap();
    return {};
}

void AppState::selectProof(const QString &id) {
    m_selectedId = id;
    m_selectedDigest.clear();
    for (const auto &entry : m_proofs)
        if (entry.toMap().value("id") == id)
            m_selectedDigest = entry.toMap().value("digest").toString();
    setPage("ProofDetail");
    emit historyChanged();
}

QString AppState::shortAddress(const QString &value) const {
    return value.size() > 18 ? value.left(8) + "…" + value.right(6) : value;
}

QString AppState::displayTime(double timestamp) const {
    return timestamp > 0 ? QDateTime::fromSecsSinceEpoch(static_cast<qint64>(timestamp))
                               .toString("MMM d, HH:mm:ss")
                         : QStringLiteral("—");
}

int AppState::passwordStrength(const QString &password) const {
    const int characters = password.toUcs4().size();
    int score = characters >= 12 ? 1 : 0;
    if (characters >= 16)
        ++score;
    if (password.contains(QRegularExpression("[A-Za-z]")) &&
        password.contains(QRegularExpression("[0-9]")))
        ++score;
    if (password.contains(QRegularExpression("[^A-Za-z0-9]")))
        ++score;
    return characters < 12 ? 0 : score;
}

void AppState::copyText(const QString &value) {
    if (value.isEmpty() || value.size() > 8192)
        return;
    QGuiApplication::clipboard()->setText(value);
    announce("Copied to clipboard.");
}

void AppState::openGuide() {
    setPage("Help");
}

QString AppState::helpText(const QString &topic) const {
    const QMap<QString, QString> files = {
        {"start", "APP_GUIDE.md"}, {"protocol", "ABOUT_ZORU.md"}, {"updates", "UPDATES.md"}};
    if (!files.contains(topic))
        return {};
    QFile file(":/docs/" + files.value(topic));
    return file.open(QIODevice::ReadOnly)
               ? QString::fromUtf8(file.readAll())
               : QStringLiteral("The guide could not be opened. Reinstall ZORU Miner.");
}

QString AppState::helpHtml(const QString &topic, const QColor &linkColor, const QFont &font) const {
    const auto text = helpText(topic);
    if (text.isEmpty() || !linkColor.isValid())
        return {};
    QTextDocument document;
    document.setDefaultFont(font);
    document.setMarkdown(text);
    struct Anchor {
        int position, length;
        QTextCharFormat format;
    };
    QList<Anchor> anchors;
    for (auto block = document.begin(); block.isValid(); block = block.next()) {
        for (auto it = block.begin(); !it.atEnd(); ++it) {
            const auto fragment = it.fragment();
            auto format = fragment.charFormat();
            if (!format.isAnchor())
                continue;
            format.setForeground(linkColor);
            anchors.append({fragment.position(), fragment.length(), format});
        }
    }
    for (const auto &anchor : anchors) {
        QTextCursor cursor(&document);
        cursor.setPosition(anchor.position);
        cursor.setPosition(anchor.position + anchor.length, QTextCursor::KeepAnchor);
        cursor.setCharFormat(anchor.format);
    }
    return document.toHtml();
}

bool AppState::eventFilter(QObject *, QEvent *event) {
    if (event->type() == QEvent::KeyPress || event->type() == QEvent::MouseButtonPress ||
        event->type() == QEvent::Wheel || event->type() == QEvent::TouchBegin)
        m_lastInteraction = QDateTime::currentMSecsSinceEpoch();
    return false;
}

void AppState::tick() {
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    if (m_unlocked && !m_busy && !m_running && now - m_lastInteraction >= m_autoLock * 60 * 1000)
        lockWallet();
    if (m_healthy && (now - m_lastSnapshot > 120000 || now / 1000 - m_blockTimestamp > 45))
        networkLost("Unsynced");
    emit clockChanged();
}

void AppState::openExplorer(const QString &kind, const QString &value) {
    if (kind == "tx"      ? !isHash(value)
        : kind == "block" ? !QRegularExpression("^[0-9]+$").match(value).hasMatch()
                          : !isAddress(value))
        return;
    const auto path = kind == "tx" ? "tx" : kind == "block" ? "block" : "address";
    QDesktopServices::openUrl(QUrl("https://vexascan.com/evm/" + QString(path) + "/" + value));
}
void AppState::resetSettings() {
    if (m_running || m_busy)
        return;
    for (const auto &group : {"mining", "connection", "security", "notifications"})
        m_preferences->remove(group);
    m_threads = suggestedThreads();
    m_intensity = 75;
    m_stopAfterProof = false;
    m_autoLock = 15;
    m_notifications = true;
    m_rpcUrl = "https://vexascan.com/rpc";
    m_rpcUrls = {m_rpcUrl};
    m_sessionBudget = "0.2";
    saveRpcPreferences();
    emit preferencesChanged();
    startMonitor();
    announce("Preferences restored.");
}
QString AppState::transactionExplanation(const QVariantMap &record) const {
    const auto status = record.value("status").toString();
    if (status == "Reward paid")
        return "The claim is recorded on-chain and the reward was transferred to this wallet.";
    if (status == "Accepted")
        return "The contract accepted this proof. Its epoch budget is shared with other accepted "
               "proofs. Claim Rewards collects eligible rewards after the epoch closes.";
    if (status == "Reorged")
        return "This proof is absent from the canonical chain. It is excluded from rewards.";
    return "Check the canonical transaction receipt and proof history for this operation.";
}

void AppState::installUpdate() {
    if (m_busy || m_updateInstallRequested || !m_updates.prepareInstall())
        return;
    m_updateInstallRequested = true;
    stopMining();
    if (m_miner.state() == QProcess::NotRunning)
        beginUpdateInstall();
}
void AppState::beginUpdateInstall() {
    m_updateInstallRequested = false;
    if (!m_updates.canInstall()) {
        m_updates.cancelPreparedInstall();
        return;
    }
    m_sessionAuthorized = false;
    erase(m_pendingCredential);
    erase(m_startCredential);
    m_unlocked = false;
    emit walletChanged();
    m_updateNetworkSuspended = m_enableNetwork;
    m_enableNetwork = false;
    m_monitorRestart.stop();
    stopProcess(m_monitor);
    m_updates.install();
}
