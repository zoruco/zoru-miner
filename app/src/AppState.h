#pragma once
#include <QObject>
#include <QLibrary>
#include <QNetworkAccessManager>
#include <QJsonObject>
#include <QTimer>
#include <QProcess>
#include <QByteArray>
#include <QVariantList>
#include <memory>
class WalletLibrary;
class AppState final:public QObject{
Q_OBJECT
Q_PROPERTY(QString version READ version CONSTANT)
Q_PROPERTY(QString network READ network NOTIFY networkChanged)
Q_PROPERTY(QString balance READ balance NOTIFY balanceChanged)
Q_PROPERTY(bool hasWallet READ hasWallet NOTIFY walletChanged)
Q_PROPERTY(bool unlocked READ unlocked NOTIFY walletChanged)
Q_PROPERTY(QString address READ address NOTIFY walletChanged)
Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
Q_PROPERTY(bool darkMode READ darkMode WRITE setDarkMode NOTIFY darkModeChanged)
Q_PROPERTY(bool engineReady READ engineReady CONSTANT)
Q_PROPERTY(bool showIntroduction READ showIntroduction NOTIFY introductionChanged)
Q_PROPERTY(bool minerAvailable READ minerAvailable CONSTANT)
Q_PROPERTY(bool mining READ mining NOTIFY miningChanged)
Q_PROPERTY(QString miningStatus READ miningStatus NOTIFY miningChanged)
Q_PROPERTY(QString hashrate READ hashrate NOTIFY statsChanged)
Q_PROPERTY(int proofsFound READ proofsFound NOTIFY statsChanged)
Q_PROPERTY(int rewardsConfirmed READ rewardsConfirmed NOTIFY statsChanged)
Q_PROPERTY(QString lastMiningEvent READ lastMiningEvent NOTIFY statsChanged)
Q_PROPERTY(int cpuThreads READ cpuThreads WRITE setCpuThreads NOTIFY cpuThreadsChanged)
Q_PROPERTY(int maxThreads READ maxThreads CONSTANT)
Q_PROPERTY(QVariantList hashrateSamples READ hashrateSamples NOTIFY statsChanged)
public:
  explicit AppState(QObject *parent=nullptr);
  ~AppState() override;
  QString version()const{return "0.1.1";}
  QString network()const{return m_network;}
  QString balance()const{return m_balance;}
  bool hasWallet()const{return m_exists;}
  bool unlocked()const{return m_unlocked;}
  QString address()const{return m_address;}
  bool busy()const{return m_busy;}
  bool darkMode()const{return m_darkMode;}
  void setDarkMode(bool value);
  bool showIntroduction()const{return !m_introductionComplete && !m_exists;}
  bool minerAvailable()const{return m_minerAvailable;}
  bool mining()const{return m_mining;}
  QString miningStatus()const{return m_miningStatus;}
  QString hashrate()const{return m_hashrate;}
  int proofsFound()const{return m_proofsFound;}
  int rewardsConfirmed()const{return m_rewardsConfirmed;}
  QString lastMiningEvent()const{return m_lastMiningEvent;}
  int cpuThreads()const{return m_threads;}
  int maxThreads()const{return m_maxThreads;}
  QVariantList hashrateSamples()const{return m_hashrateSamples;}
  void setCpuThreads(int count);
  Q_INVOKABLE void completeIntroduction();
  Q_INVOKABLE bool startMining(const QString& password);
  Q_INVOKABLE bool claimRewards(const QString& password);
  Q_INVOKABLE void stopMining();
  bool engineReady()const{return m_ready;}
  Q_INVOKABLE void createWallet(const QString& password);
  Q_INVOKABLE void importWallet(const QString& filepath,const QString& password);
  Q_INVOKABLE void importRecovery(const QString& method,const QString& secret,const QString& newPassword);
  Q_INVOKABLE void removeWallet(const QString& confirmation);
  Q_INVOKABLE void resetSettings();
  Q_INVOKABLE void unlockWallet(const QString& password);
  Q_INVOKABLE void backupWallet(const QString& destination);
  Q_INVOKABLE void lockWallet();
  Q_INVOKABLE void copyAddress();
  Q_INVOKABLE void refreshBalance();
signals:
  void networkChanged();
  void balanceChanged();
  void walletChanged();
  void busyChanged();
  void darkModeChanged();
  void introductionChanged();
  void miningChanged();
  void statsChanged();
  void cpuThreadsChanged();
  void notification(const QString& message,bool isError);
private:
  void consumeMinerOutput(const QByteArray& data);
  void onMinerLine(const QString& line);
  bool startSession(const QString& password, bool claim);
  void sendMinerCommand(const QJsonObject& command);
  enum class Action{Create,Import,Recovery,Unlock,Backup,Remove};
  void execute(Action action,const QString& file,const QString& password,const QString& method={});
  void completed(Action action,const QJsonObject& result);
  void queryNetwork();
  void requestBalance();
  std::shared_ptr<WalletLibrary> m_wallet;
  QNetworkAccessManager m_rpc;
  QString m_storage,m_address,m_network="Connecting",m_balance="—";
  QString m_cliPath,m_manifest,m_sessionAction;
  bool m_connecting=false,m_connectFailed=false,m_inspectionVerified=false;
  bool m_stopRequested=false,m_claiming=false;
  bool m_exists=false,m_unlocked=false,m_busy=false,m_ready=false,m_darkMode=true;
  bool m_introductionComplete=false,m_minerAvailable=false,m_mining=false;
  int m_threads=2,m_maxThreads=2,m_proofsFound=0,m_rewardsConfirmed=0;
  QString m_miningStatus="Standby",m_hashrate="—",m_lastMiningEvent="No mining activity";
  QProcess* m_miner=nullptr;
  QByteArray m_minerBuffer;
  QVariantList m_hashrateSamples;
};
