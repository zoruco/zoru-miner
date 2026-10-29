#pragma once
#include "AppVersion.h"
#include "UpdateService.h"
#include "WalletService.h"
#include <QColor>
#include <QDateTime>
#include <QElapsedTimer>
#include <QFont>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QSet>
#include <QSettings>
#include <QTimer>
#include <QVariantList>
#include <memory>
class AppStateTest;
class AppState final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap globalStats READ globalStats NOTIFY networkChanged)
    Q_PROPERTY(QString myPending READ myPending NOTIFY networkChanged)
    Q_PROPERTY(QString myUnclaimed READ myUnclaimed NOTIFY networkChanged)
    Q_PROPERTY(QString version READ version CONSTANT)
    Q_PROPERTY(QObject *updates READ updates CONSTANT)
    Q_PROPERTY(QString page READ page WRITE setPage NOTIFY navigationChanged)
    Q_PROPERTY(bool showIntroduction READ showIntroduction NOTIFY navigationChanged)
    Q_PROPERTY(bool hasWallet READ hasWallet NOTIFY walletChanged)
    Q_PROPERTY(bool unlocked READ unlocked NOTIFY walletChanged)
    Q_PROPERTY(bool walletDamaged READ walletDamaged NOTIFY walletChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(bool engineReady READ engineReady CONSTANT)
    Q_PROPERTY(bool backupConfirmed READ backupConfirmed NOTIFY walletChanged)
    Q_PROPERTY(QString address READ address NOTIFY walletChanged)
    Q_PROPERTY(QString network READ network NOTIFY networkChanged)
    Q_PROPERTY(bool networkHealthy READ networkHealthy NOTIFY networkChanged)
    Q_PROPERTY(QString height READ height NOTIFY networkChanged)
    Q_PROPERTY(QString latency READ latency NOTIFY networkChanged)
    Q_PROPERTY(QString blockAge READ blockAge NOTIFY clockChanged)
    Q_PROPERTY(QString tokenBalance READ tokenBalance NOTIFY networkChanged)
    Q_PROPERTY(QString nativeBalance READ nativeBalance NOTIFY networkChanged)
    Q_PROPERTY(QString totalSupply READ totalSupply NOTIFY networkChanged)
    Q_PROPERTY(QString currentReward READ currentReward NOTIFY networkChanged)
    Q_PROPERTY(QVariantMap halving READ halving NOTIFY networkChanged)
    Q_PROPERTY(QString epoch READ epoch NOTIFY networkChanged)
    Q_PROPERTY(QString difficulty READ difficulty NOTIFY networkChanged)
    Q_PROPERTY(QVariantList nodes READ nodes NOTIFY networkChanged)
    Q_PROPERTY(QVariantList contracts READ contracts NOTIFY networkChanged)
    Q_PROPERTY(QVariantList latencySamples READ latencySamples NOTIFY networkChanged)
    Q_PROPERTY(bool mining READ mining NOTIFY miningChanged)
    Q_PROPERTY(bool claiming READ claiming NOTIFY miningChanged)
    Q_PROPERTY(bool paused READ paused NOTIFY miningChanged)
    Q_PROPERTY(bool manuallyPaused READ manuallyPaused NOTIFY miningChanged)
    Q_PROPERTY(QStringList miningHolds READ miningHolds NOTIFY miningChanged)
    Q_PROPERTY(bool activelyHashing READ activelyHashing NOTIFY miningChanged)
    Q_PROPERTY(QString miningExplanation READ miningExplanation NOTIFY miningChanged)
    Q_PROPERTY(QString miningBlocker READ miningBlocker NOTIFY readinessChanged)
    Q_PROPERTY(QString miningIssue READ miningIssue NOTIFY miningChanged)
    Q_PROPERTY(QString miningStatus READ miningStatus NOTIFY miningChanged)
    Q_PROPERTY(QString processStatus READ processStatus NOTIFY miningChanged)
    Q_PROPERTY(QString networkReadiness READ networkReadiness NOTIFY readinessChanged)
    Q_PROPERTY(bool beaconVerified READ beaconVerified NOTIFY readinessChanged)
    Q_PROPERTY(bool signingAuthorized READ signingAuthorized NOTIFY readinessChanged)
    Q_PROPERTY(QString lastTransactionFeeLimit READ lastTransactionFeeLimit NOTIFY statsChanged)
    Q_PROPERTY(QString sessionFeesSpent READ sessionFeesSpent NOTIFY statsChanged)
    Q_PROPERTY(QString sessionFeesReserved READ sessionFeesReserved NOTIFY statsChanged)
    Q_PROPERTY(QString sessionFeesRemaining READ sessionFeesRemaining NOTIFY statsChanged)
    Q_PROPERTY(QString approvedSessionBudget READ approvedSessionBudget NOTIFY statsChanged)
    Q_PROPERTY(QString hashrateValue READ hashrateValue NOTIFY statsChanged)
    Q_PROPERTY(QString hashrateUnit READ hashrateUnit NOTIFY statsChanged)
    Q_PROPERTY(QString lastMeasuredHashrate READ lastMeasuredHashrate NOTIFY statsChanged)
    Q_PROPERTY(QString lastMeasuredAt READ lastMeasuredAt NOTIFY statsChanged)
    Q_PROPERTY(QString sessionHashes READ sessionHashes NOTIFY statsChanged)
    Q_PROPERTY(QVariantList hashrateSamples READ hashrateSamples NOTIFY statsChanged)
    Q_PROPERTY(qulonglong proofsFound READ proofsFound NOTIFY statsChanged)
    Q_PROPERTY(int sessionAccepted READ sessionAccepted NOTIFY statsChanged)
    Q_PROPERTY(int sessionRejected READ sessionRejected NOTIFY statsChanged)
    Q_PROPERTY(QString efficiency READ efficiency NOTIFY statsChanged)
    Q_PROPERTY(QString uptime READ uptime NOTIFY clockChanged)
    Q_PROPERTY(QString sessionStarted READ sessionStarted NOTIFY miningChanged)
    Q_PROPERTY(QVariantList proofs READ proofs NOTIFY historyChanged)
    Q_PROPERTY(QVariantMap selectedProof READ selectedProof NOTIFY historyChanged)
    Q_PROPERTY(QString confirmedRewards READ confirmedRewards NOTIFY historyChanged)
    Q_PROPERTY(qulonglong confirmedProofs READ confirmedProofs NOTIFY historyChanged)
    Q_PROPERTY(QVariantList rewardSamples READ rewardSamples NOTIFY historyChanged)
    Q_PROPERTY(QString historyStatus READ historyStatus NOTIFY historyChanged)
    Q_PROPERTY(int cpuThreads READ cpuThreads WRITE setCpuThreads NOTIFY preferencesChanged)
    Q_PROPERTY(int maxThreads READ maxThreads NOTIFY hardwareChanged)
    Q_PROPERTY(QString cpuModel READ cpuModel NOTIFY hardwareChanged)
    Q_PROPERTY(int suggestedThreads READ suggestedThreads NOTIFY hardwareChanged)
    Q_PROPERTY(int intensity READ intensity WRITE setIntensity NOTIFY preferencesChanged)
    Q_PROPERTY(
        bool stopAfterProof READ stopAfterProof WRITE setStopAfterProof NOTIFY preferencesChanged)
    Q_PROPERTY(
        int autoLockMinutes READ autoLockMinutes WRITE setAutoLockMinutes NOTIFY preferencesChanged)
    Q_PROPERTY(bool notificationsEnabled READ notificationsEnabled WRITE setNotificationsEnabled
                   NOTIFY preferencesChanged)
    Q_PROPERTY(QString rpcUrl READ rpcUrl WRITE setRpcUrl NOTIFY preferencesChanged)
    Q_PROPERTY(
        QString sessionBudget READ sessionBudget WRITE setSessionBudget NOTIFY preferencesChanged)
    Q_PROPERTY(bool pendingTransaction READ pendingTransaction NOTIFY miningChanged)
  public:
    explicit AppState(const QString &storageRoot = {}, bool enableNetwork = true,
                      QObject *parent = nullptr);
    ~AppState() override;
    QVariantMap globalStats() const { return m_globalStats; }
    QString myPending() const { return m_historyComplete ? m_myPending : QStringLiteral("—"); }
    QString myUnclaimed() const { return m_historyComplete ? m_myUnclaimed : QStringLiteral("—"); }
    QString version() const { return QStringLiteral(ZORU_APP_VERSION); }
    QObject *updates() { return &m_updates; }
    QString page() const { return m_page; }
    bool showIntroduction() const { return !m_introductionComplete && !m_hasWallet; }
    bool hasWallet() const { return m_hasWallet; }
    bool unlocked() const { return m_unlocked; }
    bool walletDamaged() const { return m_damaged; }
    bool busy() const { return m_busy; }
    bool engineReady() const { return m_wallet.ready() && m_engineAvailable; }
    bool backupConfirmed() const { return m_backupConfirmed; }
    QString address() const { return m_address; }
    QString network() const { return m_network; }
    bool networkHealthy() const { return m_healthy; }
    QString height() const { return m_height; }
    QString latency() const { return m_latency; }
    QString blockAge() const;
    QString tokenBalance() const { return m_healthy ? m_tokenBalance : QStringLiteral("—"); }
    QString nativeBalance() const { return m_healthy ? m_nativeBalance : QStringLiteral("—"); }
    QString totalSupply() const { return m_healthy ? m_totalSupply : QStringLiteral("—"); }
    QString currentReward() const { return m_healthy ? m_reward : QStringLiteral("—"); }
    QVariantMap halving() const { return m_healthy ? m_halving : QVariantMap{}; }
    QString epoch() const { return m_healthy ? m_epoch : QStringLiteral("—"); }
    QString difficulty() const { return m_healthy ? m_difficulty : QStringLiteral("—"); }
    QVariantList nodes() const;
    QVariantList contracts() const;
    QVariantList latencySamples() const { return m_latencySamples; }
    bool mining() const { return m_running; }
    bool claiming() const { return m_running && m_pendingAction == "claim"; }
    bool paused() const { return m_running && !miningHolds().isEmpty(); }
    bool manuallyPaused() const {
        return m_running && (m_manualPauseRequested || m_engineHolds.contains("manual"));
    }
    QStringList miningHolds() const;
    bool activelyHashing() const;
    QString miningExplanation() const;
    QString miningBlocker() const;
    QString miningIssue() const { return m_lastMiningIssue; }
    QString miningStatus() const { return m_miningStatus; }
    QString processStatus() const { return m_miningStatus; }
    QString networkReadiness() const;
    bool beaconVerified() const {
        return m_running && m_beaconVerified && !miningHolds().contains("network") &&
               !miningHolds().contains("security");
    }
    bool signingAuthorized() const {
        return m_running && m_engineStateKnown && m_sessionAuthorized && m_unlocked &&
               !m_stopRequested && miningHolds().isEmpty();
    }
    QString lastTransactionFeeLimit() const { return m_lastTransactionFeeLimit; }
    QString sessionFeesSpent() const { return m_sessionFeesSpent; }
    QString sessionFeesReserved() const { return m_sessionFeesReserved; }
    QString sessionFeesRemaining() const { return m_sessionFeesRemaining; }
    QString approvedSessionBudget() const;
    QString hashrateValue() const;
    QString hashrateUnit() const;
    QString lastMeasuredHashrate() const;
    QString lastMeasuredAt() const;
    QString sessionHashes() const { return m_sessionHashes; }
    QVariantList hashrateSamples() const { return m_hashrateSamples; }
    qulonglong proofsFound() const { return m_found; }
    int sessionAccepted() const;
    int sessionRejected() const;
    QString efficiency() const;
    QString uptime() const;
    QString sessionStarted() const;
    QVariantList proofs() const { return m_proofs; }
    QVariantMap selectedProof() const;
    QString confirmedRewards() const {
        return m_historyComplete ? m_confirmedRewards : QStringLiteral("—");
    }
    qulonglong confirmedProofs() const { return m_confirmedProofs; }
    QVariantList rewardSamples() const { return m_rewardSamples; }
    QString historyStatus() const { return m_historyStatus; }
    int cpuThreads() const { return m_threads; }
    int maxThreads() const { return m_maxThreads; }
    QString cpuModel() const { return m_cpuModel; }
    int suggestedThreads() const { return qMax(1, m_maxThreads / 2); }
    int intensity() const { return m_intensity; }
    bool stopAfterProof() const { return m_stopAfterProof; }
    int autoLockMinutes() const { return m_autoLock; }
    bool notificationsEnabled() const { return m_notifications; }
    QString rpcUrl() const { return m_rpcUrl; }
    QString sessionBudget() const { return m_sessionBudget; }
    bool pendingTransaction() const;
    void setPage(const QString &value);
    void setCpuThreads(int value);
    void setIntensity(int value);
    void setStopAfterProof(bool value);
    void setAutoLockMinutes(int value);
    void setNotificationsEnabled(bool value);
    void setRpcUrl(const QString &value);
    Q_INVOKABLE bool addRpc(const QString &url);
    Q_INVOKABLE bool removeRpc(const QString &url);
    void setSessionBudget(const QString &value);
    Q_INVOKABLE void completeIntroduction();
    Q_INVOKABLE void walletAction(const QString &action, const QString &password,
                                  const QString &confirmation = {}, const QString &file = {},
                                  const QString &oldPassword = {},
                                  const QString &recoverySecret = {}, const QString &method = {});
    Q_INVOKABLE bool startMining(const QString &password, bool authorized);
    Q_INVOKABLE bool claimRewards(const QString &password, bool authorized);
    Q_INVOKABLE void pauseMining();
    Q_INVOKABLE void resumeMining();
    Q_INVOKABLE void stopMining();
    Q_INVOKABLE QString transactionExplanation(const QVariantMap &record) const;
    Q_INVOKABLE void lockWallet();
    Q_INVOKABLE void selectProof(const QString &id);
    Q_INVOKABLE void copyText(const QString &value);
    Q_INVOKABLE void openExplorer(const QString &kind, const QString &value);
    Q_INVOKABLE void openGuide();
    Q_INVOKABLE QString helpHtml(const QString &topic, const QColor &linkColor,
                                 const QFont &font) const;
    Q_INVOKABLE void installUpdate();
    Q_INVOKABLE void resetSettings();
    Q_INVOKABLE QString shortAddress(const QString &value) const;
    Q_INVOKABLE QString displayTime(double timestamp) const;
    Q_INVOKABLE int passwordStrength(const QString &password) const;
    bool eventFilter(QObject *, QEvent *) override;
  signals:
    void navigationChanged();
    void walletChanged();
    void busyChanged();
    void networkChanged();
    void clockChanged();
    void readinessChanged();
    void hardwareChanged();
    void miningChanged();
    void statsChanged();
    void historyChanged();
    void preferencesChanged();
    void notification(const QString &message, bool error);
    void recoveryDetails(const QString &kind, const QString &secret);
    void walletActionFinished(const QString &action, bool success, const QString &message);

  private:
    friend class AppStateTest;
    void loadWallet();
    void startMonitor();
    void stopProcess(QProcess &process);
    void consume(QProcess &process, QByteArray &buffer, bool miner);
    void consumeFrames(QByteArray &buffer, bool miner);
    void applyMonitorEvent(const QJsonObject &event);
    void applyMinerEvent(const QJsonObject &event);
    void applyInspection(const QJsonObject &data);
    void applyFees(const QJsonObject &fees);
    void completed(const QString &action, const QJsonObject &result);
    void launchMiner(QByteArray password);
    void beginUpdateInstall();
    void clearWalletProjection();
    void announce(const QString &message, bool error = false);
    void tick();
    bool sendControl(const QString &action);
    bool authorizeSession(const QString &password, bool authorized);
    void networkLost(const QString &state, bool security = false);
    void finishSession();
    void saveRpcPreferences();
    QString helpText(const QString &topic) const;
    QString enginePath() const;
    QString manifestPath() const;
    QString budgetWei() const;
    QString statePath() const;
    QStringList connectionArguments() const;
    std::unique_ptr<QSettings> m_preferences;
    WalletService m_wallet;
    UpdateService m_updates;
    QByteArray m_pendingCredential, m_startCredential, m_monitorBuffer, m_minerBuffer;
    QProcess m_monitor, m_miner;
    QTimer m_clock, m_monitorRestart;
    QVariantMap m_halving, m_globalStats;
    QString m_storage, m_page = "Overview", m_address, m_rpcUrl, m_activeRpc,
                       m_sessionBudget = "0.2";
    QString m_deploymentScope, m_monitorAddress, m_monitorRpc, m_pendingAction;
    QStringList m_rpcUrls;
    QString m_network = "Connecting", m_height = "—", m_latency = "—", m_tokenBalance = "—",
            m_nativeBalance = "—", m_totalSupply = "—", m_reward = "—", m_epoch = "—",
            m_difficulty = "—";
    QString m_miningStatus = "Standby", m_historyStatus = "Set up a wallet to view your activity",
            m_confirmedRewards = "—", m_selectedId, m_selectedDigest;
    QString m_myPending = "—", m_myUnclaimed = "—", m_lastMiningIssue, m_sessionEndStatus;
    QString m_lastTransactionFeeLimit = "—", m_sessionFeesSpent = "—", m_sessionFeesReserved = "—",
            m_sessionFeesRemaining = "—", m_sessionFeeLimit = "—";
    QString m_roundEpoch, m_cpuModel, m_approvedBudget, m_authorizedAddress, m_sessionHashes = "0";
    QSet<QString> m_engineHolds;
    quint64 m_authorizationEpoch = 0, m_walletAuthorizationEpoch = 0, m_lockEpoch = 0,
            m_walletLockEpoch = 0;
    bool m_beaconVerified = false, m_engineStateKnown = false, m_stopAfterProof = false,
         m_chainMiningPaused = false, m_claimsPaused = false;
    bool m_sessionAuthorized = false, m_manualPauseRequested = false, m_securityHeld = false,
         m_hashing = false, m_monitorRestartPending = false;
    bool m_updateInstallRequested = false, m_updateNetworkSuspended = false;
    bool m_stopRequested = false, m_enableNetwork = true, m_engineAvailable = false,
         m_introductionComplete = false, m_hasWallet = false, m_unlocked = false, m_damaged = false,
         m_busy = false, m_backupConfirmed = false, m_healthy = false, m_running = false,
         m_historyComplete = false, m_notifications = true, m_closing = false,
         m_signAfterUnlock = false;
    int m_monitorFailures = 0, m_threads = 2, m_maxThreads = 2, m_intensity = 75, m_autoLock = 15,
        m_sessionAccepted = 0, m_sessionRejected = 0;
    qulonglong m_found = 0, m_confirmedProofs = 0;
    qint64 m_lastMeasuredAt = 0, m_blockTimestamp = 0, m_lastInteraction = 0, m_started = 0,
           m_sessionEnded = 0, m_lastSnapshot = 0;
    double m_hashrate = 0, m_lastHashrate = -1;
    QVariantList m_nodes, m_contracts, m_latencySamples, m_hashrateSamples, m_proofs,
        m_rewardSamples;
    QString m_lastNotice;
    bool m_lastNoticeError = false;
    QElapsedTimer m_noticeClock;
};
