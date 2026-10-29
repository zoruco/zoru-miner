#include "AppState.h"
#include <QCoreApplication>
#include <QGuiApplication>
#include <QClipboard>
#include <QDir>
#include <QFileInfo>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QJsonParseError>
#include <QPointer>
#include <QStandardPaths>
#include <QSettings>
#include <QRegularExpression>
#include <QThread>
#include <QProcess>
#include <QFile>
#include <QSysInfo>
#include <QTimer>
#include <QDebug>
#if defined(Q_OS_UNIX)
#include <signal.h>
#endif
#include <QThread>
#include <QTimer>
#include <QUrl>
#include <QUrlQuery>
#include <algorithm>

using F1=char*(*)(const char*);
using F2=char*(*)(const char*,const char*);
using F3=char*(*)(const char*,const char*,const char*);
using F4=char*(*)(const char*,const char*,const char*,const char*);
using Free=void(*)(char*);
static void erase(QByteArray& b){
  volatile char *p=b.data();
  for(qsizetype i=0;i<b.size();++i) p[i]=0;
  b.clear();
}
class WalletLibrary {
public:
  WalletLibrary(){
#if defined(Q_OS_WIN)
    const QString filename="zoru_wallet.dll";
#elif defined(Q_OS_MACOS)
    const QString filename="libzoru_wallet.dylib";
#else
    const QString filename="libzoru_wallet.so";
#endif
    auto path=QDir(QCoreApplication::applicationDirPath()).filePath(filename);
#ifdef Q_OS_MACOS
    path=QDir(QCoreApplication::applicationDirPath()).filePath("../Frameworks/"+filename);
#endif
    if(!QFileInfo::exists(path))return;
    lib.setFileName(path);
    if(!lib.load())return;
    status=reinterpret_cast<F1>(lib.resolve("zoru_wallet_status"));
    create=reinterpret_cast<F2>(lib.resolve("zoru_wallet_create"));
    importWallet=reinterpret_cast<F3>(lib.resolve("zoru_wallet_import"));
    unlock=reinterpret_cast<F2>(lib.resolve("zoru_wallet_unlock"));
    backup=reinterpret_cast<F2>(lib.resolve("zoru_wallet_backup"));
    recovery=reinterpret_cast<F4>(lib.resolve("zoru_wallet_import_recovery"));
    removeWallet=reinterpret_cast<F2>(lib.resolve("zoru_wallet_remove"));
    release=reinterpret_cast<Free>(lib.resolve("zoru_wallet_string_free"));
    ready=status&&create&&importWallet&&recovery&&removeWallet&&unlock&&backup&&release;
  }
  QJsonObject call(char* ptr)const{
    if(!ptr)return{{"ok",false},{"error","Wallet unavailable"}};
    QByteArray json(ptr);
    release(ptr);
    const auto result=QJsonDocument::fromJson(json);
    erase(json);
    if(!result.isObject())return{{"ok",false},{"error","Unable to open wallet"}};
    return result.object();
  }
  QLibrary lib;
  F1 status=nullptr;
  F2 create=nullptr,unlock=nullptr,backup=nullptr;
  F3 importWallet=nullptr;
  F4 recovery=nullptr;
  F2 removeWallet=nullptr;
  Free release=nullptr;
  bool ready=false;
};
AppState::AppState(QObject* parent):QObject(parent),m_wallet(std::make_shared<WalletLibrary>()),m_rpc(this){
  QSettings settings;
  m_darkMode=settings.value("appearance/darkMode",true).toBool();
  m_introductionComplete=settings.value("introduction/complete",false).toBool();
  // The audited mainnet engine permits 1, 2 or 4 workers only.
  m_maxThreads=qBound(1,QThread::idealThreadCount(),4);
  if(m_maxThreads==3)m_maxThreads=2;
  m_threads=qBound(1,settings.value("mining/threads",qMin(2,m_maxThreads)).toInt(),m_maxThreads);
  if(m_threads==3)m_threads=2;
  const QString dir=QCoreApplication::applicationDirPath();
  m_cliPath=QDir(dir).filePath(
#if defined(Q_OS_WIN)
    "zoru-miner-cli.exe"
#else
    "zoru-miner-cli"
#endif
  );
  for(const QString& candidate:{
      QDir(dir).filePath("../deployment.json"),
      QDir(dir).filePath("deployment.json"),
      QDir(dir).filePath("../release/mainnet.json")
  }){
    if(QFileInfo(candidate).isFile()){m_manifest=QDir::cleanPath(candidate);break;}
  }
  m_minerAvailable=QFileInfo(m_cliPath).isExecutable() && !m_manifest.isEmpty();
  m_miner=new QProcess(this);
  m_miner->setProcessChannelMode(QProcess::SeparateChannels);
  QObject::connect(m_miner,&QProcess::readyReadStandardOutput,this,[this]{
      consumeMinerOutput(m_miner->readAllStandardOutput());
  });
  QObject::connect(m_miner,&QProcess::readyReadStandardError,this,[this]{
      // Never echo raw backend stderr or credentials into the public UI.
      m_miner->readAllStandardError();
  });
  QObject::connect(m_miner,QOverload<int,QProcess::ExitStatus>::of(&QProcess::finished),this,
    [this](int code,QProcess::ExitStatus state){
      m_mining=false;m_claiming=false;m_connecting=false;m_miningStatus="Standby";
      m_hashrate="—";m_busy=false;
      emit miningChanged();emit statsChanged();emit busyChanged();
      if(!m_minerBuffer.isEmpty()){onMinerLine(QString::fromUtf8(m_minerBuffer));m_minerBuffer.clear();}
      if(code!=0||state!=QProcess::NormalExit)
        emit notification("The verified mainnet backend stopped unexpectedly.",true);
    });
  QObject::connect(m_miner,&QProcess::errorOccurred,this,[this](QProcess::ProcessError){
      m_mining=false;m_claiming=false;m_connecting=false;m_miningStatus="Standby";m_busy=false;
      emit miningChanged();emit busyChanged();
      emit notification("Unable to start the mainnet mining engine.",true);
  });
  m_ready=m_wallet->ready;
  m_storage=QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
  if(m_ready){
    QByteArray b=m_storage.toUtf8();
    const auto response=m_wallet->call(m_wallet->status(b.constData()));
    erase(b);
    m_exists=QFileInfo(QDir(m_storage).filePath("mining-wallet.json")).isFile();
    if(m_exists && !response.value("ok").toBool())
      QTimer::singleShot(0,this,[this]{emit notification("Wallet data could not be opened. Restore a backup or remove the local wallet in Settings.",true);});
  }
  queryNetwork();
  auto *timer=new QTimer(this);
  timer->setInterval(45000);
  connect(timer,&QTimer::timeout,this,&AppState::queryNetwork);
  timer->start();
}
AppState::~AppState(){
  if(m_miner && m_miner->state()!=QProcess::NotRunning){
    sendMinerCommand({{"action","stop"}});
    sendMinerCommand({{"action","quit"}});
    if(!m_miner->waitForFinished(30000)){
      // A pending transaction is journaled by the Rust backend. Do not
      // deliberately kill the writer during a signed transaction.
      m_miner->waitForFinished(30000);
    }
  }
}
void AppState::completeIntroduction(){
  if(m_introductionComplete)return;
  m_introductionComplete=true;
  QSettings().setValue("introduction/complete",true);
  emit introductionChanged();
}
void AppState::setCpuThreads(int count){
  count=qBound(1,count,m_maxThreads);
  if(count==3)count=m_threads==2?4:2;
  if(count==m_threads)return;
  if(m_mining||m_claiming||m_connecting){
    emit notification("Stop the active session before changing CPU workers.",true);return;
  }
  m_threads=count;
  QSettings().setValue("mining/threads",count);
  emit cpuThreadsChanged();
}
void AppState::consumeMinerOutput(const QByteArray& data){
  m_minerBuffer.append(data);
  if(m_minerBuffer.size()>65536)m_minerBuffer=m_minerBuffer.right(2048);
  while(true){
    int n=m_minerBuffer.indexOf('\n');
    if(n<0)break;
    const QString line=QString::fromUtf8(m_minerBuffer.left(n)).trimmed();
    m_minerBuffer.remove(0,n+1);
    if(!line.isEmpty())onMinerLine(line);
  }
}
void AppState::sendMinerCommand(const QJsonObject& command){
  if(m_miner->state()!=QProcess::Running)return;
  QByteArray line=QJsonDocument(command).toJson(QJsonDocument::Compact);
  line.append('\n');
  m_miner->write(line);
  erase(line);
}
void AppState::onMinerLine(const QString& line){
  QJsonParseError parse;
  const auto doc=QJsonDocument::fromJson(line.toUtf8(),&parse);
  if(parse.error!=QJsonParseError::NoError||!doc.isObject())return;
  const auto message=doc.object();
  const QString event=message.value("event").toString();
  const auto data=message.value("data").toObject();
  if(event=="error"){
    m_lastMiningEvent=message.value("error").toString("Mainnet backend error");
    if(m_connecting)m_connectFailed=true;
    emit statsChanged();
    emit notification(m_lastMiningEvent,true);
    return;
  }
  if(event=="warning"){
    m_lastMiningEvent=message.value("warning").toString("Miner warning");
    emit statsChanged();
    return;
  }
  if(event=="inspection"){
    // Both chain identity and bytecode fingerprints are checked in Rust
    // before Session::inspect produces this event.
    if(data.value("chainId").toInt()!=6736){
      m_connectFailed=true;
      emit notification("Mainnet chain identity mismatch.",true);
    }else{
      m_inspectionVerified=true;
    }
    return;
  }
  if(event=="state"||event=="mine-result"){
    const QString status=data.value("status").toString();
    if(data.contains("hashesPerSecond")){
      const double rate=data.value("hashesPerSecond").toDouble();
      m_hashrate=QString::number(rate,'f',1);
      m_hashrateSamples.append(rate);
      if(m_hashrateSamples.size()>70)m_hashrateSamples.removeFirst();
    }
    if(data.contains("validFound"))m_proofsFound=data.value("validFound").toInt();
    if(data.contains("accepted"))m_rewardsConfirmed=data.value("accepted").toInt();
    if(status=="paused")m_miningStatus="Paused";
    else if(status=="paused-resource")m_miningStatus="CPU busy";
    else if(status=="waiting-beacon-or-recovery")m_miningStatus="Waiting for beacon";
    else if(status=="mining")m_miningStatus="Mining";
    else if(status=="stopped")m_miningStatus="Standby";
    else if(m_connecting)m_miningStatus="Verifying";
    if(data.value("accepted").toInt()>0)
      m_lastMiningEvent="Accepted proof confirmed on-chain";
    else if(data.value("validFound").toInt()>0)
      m_lastMiningEvent="Proof found — waiting for receipt";
    else if(status=="mining")
      m_lastMiningEvent="Hashing against verified mainnet work";
    emit statsChanged();emit miningChanged();
    if(event=="mine-result")requestBalance();
    return;
  }
  if(event=="claims"){
    m_lastMiningEvent="Claim finished; check confirmed reward balances";
    emit statsChanged();requestBalance();
    emit notification("Claim action completed. Inspect rewards for on-chain confirmation.",false);
    return;
  }
  if(event=="command-complete"){
    const QString action=message.value("action").toString();
    if(action=="connect"){
      m_connecting=false;
      if(!m_connectFailed && m_inspectionVerified && !m_stopRequested){
        if(m_sessionAction=="mine"){
          m_miningStatus="Starting";emit miningChanged();
          sendMinerCommand({{"action","mine"},{"workers",m_threads},{"cpuBudget",0.5}});
        }else if(m_sessionAction=="claim"){
          sendMinerCommand({{"action","claim"}});
        }
      }else{
        m_mining=false;m_claiming=false;m_busy=false;
        m_miningStatus="Standby";
        emit miningChanged();emit busyChanged();
        sendMinerCommand({{"action","quit"}});
      }
    }else if(action=="mine"||action=="claim"){
      if(action=="mine"){
        m_mining=false;m_miningStatus="Standby";m_hashrate="—";
        emit miningChanged();emit statsChanged();
      }
      if(action=="claim"){m_claiming=false;m_busy=false;emit busyChanged();}
      sendMinerCommand({{"action","quit"}});
    }
  }
}
bool AppState::startSession(const QString& password,bool claim){
  if(m_miner->state()!=QProcess::NotRunning||m_connecting||m_mining||m_claiming)return false;
  if(!m_minerAvailable){emit notification("Verified mainnet engine is not installed.",true);return false;}
  if(!m_exists||!m_ready){emit notification("Create or import a wallet first.",true);return false;}
  if(m_network!="Connected"){emit notification("Connect to the VEX mainnet first.",true);return false;}
  if(password.isEmpty()){emit notification("Enter your wallet password.",true);return false;}
  // Validate the wallet using the restored native wallet service; the Rust
  // signing session will independently validate it and the deployment.
  QByteArray root=m_storage.toUtf8(),credential=password.toUtf8();
  const QJsonObject auth=m_wallet->call(m_wallet->unlock(root.constData(),credential.constData()));
  erase(root);erase(credential);
  if(!auth.value("ok").toBool()){
    emit notification("Incorrect wallet password or invalid keystore.",true);return false;
  }
  const QString key=QDir(m_storage).filePath("mining-wallet.json");
  if(!QFileInfo(key).isFile()||!QFileInfo(m_manifest).isFile())return false;
  m_sessionAction=claim?"claim":"mine";
  m_connecting=true;m_connectFailed=false;m_inspectionVerified=false;m_stopRequested=false;
  m_minerBuffer.clear();
  m_lastMiningEvent="Verifying mainnet contracts and wallet";
  if(!claim){
    m_proofsFound=0;m_rewardsConfirmed=0;m_hashrateSamples.clear();m_hashrate="—";
    m_mining=true;m_miningStatus="Verifying";emit miningChanged();
  }else{
    m_claiming=true;m_busy=true;emit busyChanged();
  }
  m_miner->setProgram(m_cliPath);
  m_miner->setArguments({"serve","--manifest",m_manifest,
    "--read","https://vexascan.com/rpc","--submit","https://vexascan.com/rpc",
    "--native","https://vexascan.com:8443",
    "--wallet",key,"--state-dir",QDir(m_storage).filePath("miner-state")});
  m_miner->start();
  if(!m_miner->waitForStarted(3500)){
    m_connecting=false;m_mining=false;m_claiming=false;m_busy=false;
    emit miningChanged();emit busyChanged();
    emit notification("Could not start the mainnet backend.",true);
    return false;
  }
  sendMinerCommand({{"action","connect"},{"password",password}});
  emit statsChanged();
  return true;
}
bool AppState::startMining(const QString& password){return startSession(password,false);}
bool AppState::claimRewards(const QString& password){return startSession(password,true);}
void AppState::stopMining(){
  if(!m_miner||m_miner->state()==QProcess::NotRunning)return;
  m_stopRequested=true;
  m_miningStatus="Stopping";emit miningChanged();
  sendMinerCommand({{"action","stop"}});
}
void AppState::setDarkMode(bool value){
  if(m_darkMode==value)return;
  m_darkMode=value;
  QSettings().setValue("appearance/darkMode",m_darkMode);
  emit darkModeChanged();
}
void AppState::createWallet(const QString& password){execute(Action::Create,{},password);}
void AppState::importWallet(const QString& file,const QString& password){execute(Action::Import,QUrl(file).isLocalFile()?QUrl(file).toLocalFile():file,password);}
void AppState::unlockWallet(const QString& password){execute(Action::Unlock,{},password);}
void AppState::importRecovery(const QString& method,const QString& secret,const QString& newPassword){
  if(method!="private_key" && method!="recovery_phrase"){
    emit notification("Invalid import method",true);return;
  }
  execute(Action::Recovery,secret,newPassword,method);
}
void AppState::removeWallet(const QString& confirmation){
  if(m_mining||m_miner->state()!=QProcess::NotRunning){
    emit notification("Stop mining before removing the wallet.",true);return;
  }
  execute(Action::Remove,{},confirmation);
}
void AppState::resetSettings(){
  if(m_busy){emit notification("Finish the current wallet operation first.",true);return;}
  if(m_mining){emit notification("Stop mining before resetting preferences.",true);return;}
  QSettings settings;
  settings.remove("appearance");
  settings.remove("mining/threads");
  m_darkMode=true;
  m_threads=qMin(2,m_maxThreads);
  emit darkModeChanged();
  emit cpuThreadsChanged();
  emit notification("Appearance and CPU preferences restored.",false);
}
void AppState::backupWallet(const QString& destination){execute(Action::Backup,QUrl(destination).isLocalFile()?QUrl(destination).toLocalFile():destination,{});}
void AppState::execute(Action action,const QString& file,const QString& password,const QString& method){
  if(m_busy)return;
  if(!m_ready){emit notification("Wallet service is unavailable",true);return;}
  if(action!=Action::Backup&&password.isEmpty()){emit notification("Enter your wallet password",true);return;}
  if((action==Action::Create||action==Action::Import||action==Action::Recovery)&&m_exists){
    emit notification("A wallet already exists on this device",true);return;
  }
  m_busy=true;emit busyChanged();
  auto lib=m_wallet;
  const QString dir=m_storage;
  const QPointer<AppState> target(this);
  auto *thread=QThread::create([lib,dir,file,password,method,action,target]{
    QByteArray root=dir.toUtf8(),src=file.toUtf8(),secret=password.toUtf8(),kind=method.toUtf8();
    QJsonObject result;
    switch(action){
      case Action::Create:result=lib->call(lib->create(root.constData(),secret.constData()));break;
      case Action::Import:result=lib->call(lib->importWallet(root.constData(),src.constData(),secret.constData()));break;
      case Action::Recovery:result=lib->call(lib->recovery(root.constData(),kind.constData(),src.constData(),secret.constData()));break;
      case Action::Remove:result=lib->call(lib->removeWallet(root.constData(),secret.constData()));break;
      case Action::Unlock:result=lib->call(lib->unlock(root.constData(),secret.constData()));break;
      case Action::Backup:result=lib->call(lib->backup(root.constData(),src.constData()));break;
    }
    erase(secret);erase(root);erase(src);erase(kind);
    if(target)QMetaObject::invokeMethod(target.data(),[target,action,result]{
      if(target)target->completed(action,result);
    },Qt::QueuedConnection);
  });
  connect(thread,&QThread::finished,thread,&QObject::deleteLater);
  thread->start();
}
void AppState::completed(Action action,const QJsonObject& result){
  m_busy=false;emit busyChanged();
  if(!result.value("ok").toBool()){
    emit notification(result.value("error").toString("This action could not be completed"),true);return;
  }
  if(action==Action::Remove){
    m_exists=false;m_unlocked=false;m_address.clear();m_balance="—";
    m_hashrate="—";m_proofsFound=0;m_rewardsConfirmed=0;m_hashrateSamples.clear();
    emit walletChanged();emit balanceChanged();emit statsChanged();
    emit notification("Local wallet removed. Your external backup is unchanged.",false);
    return;
  }
  if(action!=Action::Backup){
    m_exists=true;m_unlocked=true;
    emit introductionChanged();
    m_address=result.value("data").toObject().value("address").toString();
    m_balance="—";emit walletChanged();emit balanceChanged();
    requestBalance();
  }
  switch(action){
    case Action::Create:emit notification("Wallet created. Save an encrypted backup.",false);break;
    case Action::Import:emit notification("Encrypted wallet imported and unlocked.",false);break;
    case Action::Recovery:emit notification("Wallet imported and encrypted on this device. Save a backup.",false);break;
    case Action::Remove:break;
    case Action::Unlock:emit notification("Wallet unlocked.",false);break;
    case Action::Backup:emit notification("Encrypted backup saved.",false);break;
  }
}
void AppState::lockWallet(){
  m_unlocked=false;m_address.clear();m_balance="—";
  emit walletChanged();emit balanceChanged();
  emit notification("Wallet locked.",false);
}
void AppState::copyAddress(){
  if(!m_unlocked||m_address.isEmpty()){emit notification("Unlock your wallet first.",true);return;}
  QGuiApplication::clipboard()->setText(m_address);
  emit notification("Wallet address copied.",false);
}
static QByteArray rpcPayload(const char *method,QJsonArray params){
  return QJsonDocument(QJsonObject{{"jsonrpc","2.0"},{"id",1},{"method",QString::fromUtf8(method)},{"params",params}}).toJson(QJsonDocument::Compact);
}
static QNetworkRequest request(){
  QNetworkRequest r(QUrl("https://vexascan.com/rpc"));
  r.setHeader(QNetworkRequest::ContentTypeHeader,"application/json");
  r.setTransferTimeout(10000);
  return r;
}
void AppState::queryNetwork(){
  auto *reply=m_rpc.post(request(),rpcPayload("eth_chainId",{}));
  connect(reply,&QNetworkReply::finished,this,[this,reply]{
    const auto json=QJsonDocument::fromJson(reply->readAll()).object();
    bool online=reply->error()==QNetworkReply::NoError&&json.value("result").toString().toLower()=="0x1a50";
    m_network=online?"Connected":"Offline";
    emit networkChanged();
    reply->deleteLater();
    if(online)requestBalance();
  });
}
static QString formatWei(const QString& hex){
  if(!hex.startsWith("0x")||hex.size()<3||hex.size()>66)return {};
  QString digits="0";
  for(const QChar digit:hex.mid(2)){
    bool valid=false;
    const int val=QString(digit).toInt(&valid,16);
    if(!valid)return {};
    int carry=val;
    for(int i=digits.size()-1;i>=0;--i){
      const int n=(digits.at(i).unicode()-'0')*16+carry;
      digits[i]=QChar('0'+(n%10));
      carry=n/10;
    }
    while(carry){digits.prepend(QChar('0'+(carry%10)));carry/=10;}
  }
  while(digits.size()<=18)digits.prepend('0');
  auto integer=digits.left(digits.size()-18);
  auto fraction=digits.right(18).left(6);
  while(fraction.endsWith('0')&&!fraction.isEmpty())fraction.chop(1);
  return fraction.isEmpty()?integer:integer+"."+fraction;
}
void AppState::requestBalance(){
  if(!m_unlocked||m_address.isEmpty()||m_network!="Connected")return;
  const auto currentAddress=m_address;
  auto *reply=m_rpc.post(request(),rpcPayload("eth_getBalance",QJsonArray{currentAddress,"latest"}));
  connect(reply,&QNetworkReply::finished,this,[this,reply,currentAddress]{
    const auto json=QJsonDocument::fromJson(reply->readAll()).object();
    if(reply->error()==QNetworkReply::NoError&&m_unlocked&&m_address==currentAddress){
      const auto balance=formatWei(json.value("result").toString());
      if(!balance.isEmpty()){m_balance=balance;emit balanceChanged();}
    }
    reply->deleteLater();
  });
}
void AppState::refreshBalance(){requestBalance();}
