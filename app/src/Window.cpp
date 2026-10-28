#include "Window.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFormLayout>
#include <QGridLayout>
#include <QGroupBox>
#include <QHeaderView>
#include <QJsonDocument>
#include <QJsonArray>
#include <QCloseEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QStandardPaths>
#include <QTimer>

static QString amount(QString raw) {
    while (raw.size() < 9) raw.prepend('0');
    raw.insert(raw.size() - 8, '.');
    return raw;
}

Window::Window(QString executable, QWidget *parent) : QWidget(parent), backend(std::move(executable)) {
    setWindowTitle("ZORU Miner · VEX Mainnet"); resize(1100, 870);
    setStyleSheet("QWidget{background:#121820;color:#e3ebf1;font-size:13px} QLineEdit,QComboBox,QSpinBox,QDoubleSpinBox,QTableWidget{background:#1c2733;border:1px solid #354657;border-radius:4px;padding:5px} QPushButton{background:#244d65;border:1px solid #4e809b;border-radius:5px;padding:9px} QPushButton:disabled{color:#7b8997;background:#202b35} QGroupBox{border:1px solid #354657;border-radius:6px;margin-top:12px;padding-top:14px} QGroupBox::title{subcontrol-origin:margin;left:12px} QHeaderView::section{background:#273747;padding:5px}");
    auto *layout = new QVBoxLayout(this);
    auto *title = new QLabel("ZORU  /  CPU MINER"); title->setStyleSheet("font-size:25px;font-weight:600;color:#7fd0ea"); layout->addWidget(title);
    auto *note = new QLabel("VEX mainnet · Chain 6736 · Verify the release manifest before mining"); layout->addWidget(note);
    auto *connection = new QGroupBox("Connection & wallet"); auto *form = new QFormLayout(connection);
    const QList<QPair<QString,QString>> entries = {{"manifest","Deployment manifest"},{"read","Read RPC"},{"submit","Submit RPC"},{"native","Native RPC"},{"wallet","Encrypted wallet"},{"password","Wallet password"},{"state-dir","Local state directory"}};
    for (const auto &entry : entries) {
        auto *edit = new QLineEdit; edit->setObjectName(entry.first); fields[entry.first] = edit;
        if (entry.first == "password") edit->setEchoMode(QLineEdit::Password);
        if (entry.first == "state-dir") edit->setText(QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation));
        if (entry.first == "read") edit->setPlaceholderText("https://your-evm-rpc.example");
        if (entry.first == "submit") edit->setPlaceholderText("https://your-submit-rpc.example");
        if (entry.first == "native") edit->setPlaceholderText("https://your-native-rpc.example");
        form->addRow(entry.second, edit);
    }
    layout->addWidget(connection);
    auto *actions = new QHBoxLayout;
    connectButton = new QPushButton("Connect"); connectButton->setObjectName("connect"); actions->addWidget(connectButton);
    refresh = new QPushButton("Refresh balances"); refresh->setObjectName("refresh"); actions->addWidget(refresh);
    workers = new QComboBox; workers->setObjectName("workers"); workers->addItems({"1", "2", "4"}); actions->addWidget(new QLabel("Workers")); actions->addWidget(workers);
    budget = new QDoubleSpinBox; budget->setObjectName("cpu-budget"); budget->setRange(0.1, 2); budget->setSingleStep(0.1); budget->setValue(0.5); actions->addWidget(new QLabel("CPU cores budget")); actions->addWidget(budget);
    mine = new QPushButton("Start mining"); mine->setObjectName("mine"); actions->addWidget(mine);
    pause = new QPushButton("Pause / resume"); pause->setObjectName("pause"); actions->addWidget(pause);
    stop = new QPushButton("Stop"); stop->setObjectName("stop"); actions->addWidget(stop);
    claim = new QPushButton("Claim rewards"); claim->setObjectName("claim"); actions->addWidget(claim); layout->addLayout(actions);
    status = new QLabel("Disconnected"); status->setObjectName("status"); layout->addWidget(status);
    error = new QLabel; error->setObjectName("error"); error->setWordWrap(true); error->setStyleSheet("color:#ffaf95"); layout->addWidget(error);
    metrics = new QLabel("0 hashes · 0 accepted proofs"); metrics->setObjectName("metrics"); layout->addWidget(metrics);
    balance = new QLabel; balance->setObjectName("balance"); layout->addWidget(balance);

    auto *myRewards = new QGroupBox("My mining rewards");
    auto *myGrid = new QGridLayout(myRewards);
    auto addReward = [](QGridLayout *grid, const QString &title, const QString &object, int column) {
        auto *caption = new QLabel(title);
        caption->setStyleSheet("color:#9bb1bf");
        auto *value = new QLabel("—");
        value->setObjectName(object);
        value->setStyleSheet("color:#7fd0ea;font-size:19px;font-weight:600");
        grid->addWidget(caption, 0, column);
        grid->addWidget(value, 1, column);
        return value;
    };
    myPending = addReward(myGrid, "Pending settlement", "myPending", 0);
    myUnclaimed = addReward(myGrid, "Unclaimed", "myUnclaimed", 1);
    myClaimed = addReward(myGrid, "Claimed", "myClaimed", 2);
    myHistoryNote = new QLabel("Connect a wallet to view its proof rewards");
    myHistoryNote->setObjectName("myHistoryNote");
    myHistoryNote->setStyleSheet("color:#9bb1bf");
    myGrid->addWidget(myHistoryNote, 2, 0, 1, 3);
    layout->addWidget(myRewards);

    auto *networkRewards = new QGroupBox("Global ZORU network");
    auto *networkGrid = new QGridLayout(networkRewards);
    globalAllocated = addReward(networkGrid, "Allocated", "globalAllocated", 0);
    globalPending = addReward(networkGrid, "Pending settlement", "globalPending", 1);
    globalUnclaimed = addReward(networkGrid, "Unclaimed", "globalUnclaimed", 2);
    globalClaimed = addReward(networkGrid, "Claimed", "globalClaimed", 3);
    globalMinted = addReward(networkGrid, "Minted supply", "globalMinted", 4);
    networkInfo = new QLabel("Connect to read current epoch, accepted proofs and claim status");
    networkInfo->setObjectName("networkInfo");
    networkInfo->setStyleSheet("color:#9bb1bf");
    networkGrid->addWidget(networkInfo, 2, 0, 1, 5);
    layout->addWidget(networkRewards);

    deployment = new QLabel; deployment->setWordWrap(true); deployment->setTextInteractionFlags(Qt::TextSelectableByMouse); layout->addWidget(deployment);
    proofs = new QTableWidget(0, 4); proofs->setObjectName("proofs"); proofs->setHorizontalHeaderLabels({"Epoch", "Generation", "Transaction", "Gas used"}); proofs->horizontalHeader()->setSectionResizeMode(2, QHeaderView::Stretch); proofs->setEditTriggers(QAbstractItemView::NoEditTriggers); layout->addWidget(proofs, 1);
    auto *rewardNote = new QLabel("Accepted proofs share an epoch budget. Rewards become claimable after the epoch closes. Publishing drand, submitting, sealing and claiming each spend VEX gas."); rewardNote->setWordWrap(true); layout->addWidget(rewardNote);
    connect(connectButton, &QPushButton::clicked, this, &Window::connectBackend);
    connect(mine, &QPushButton::clicked, this, [this]{ busy = true; error->clear(); send({{"action","mine"},{"workers", workers->currentText().toInt()},{"cpuBudget", budget->value()}}); controls(); });
    connect(stop, &QPushButton::clicked, this, [this]{ send({{"action","stop"}}); status->setText("Stopping; tracking pending receipt…"); });
    connect(pause, &QPushButton::clicked, this, [this]{ send({{"action", current.value("status").toString() == "paused" ? "resume" : "pause"}}); });
    connect(claim, &QPushButton::clicked, this, [this]{ busy = true; send({{"action","claim"}}); controls(); });
    connect(refresh, &QPushButton::clicked, this, [this]{ busy = true; send({{"action","inspect"}}); controls(); });
    connect(&process, &QProcess::readyReadStandardOutput, this, &Window::consume);
    connect(&process, &QProcess::readyReadStandardError, this, [this]{ process.readAllStandardError(); error->setText("Backend reported an error. Check its installation and reconnect."); });
    connect(&process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError){ error->setText("Could not run the mining backend. Check the backend executable."); busy = false; verified = false; connecting = false; controls(); });
    connect(&process, qOverload<int,QProcess::ExitStatus>(&QProcess::finished), this, [this](int, QProcess::ExitStatus){ busy = false; verified = false; connecting = false; current = {}; status->setText("Disconnected"); controls(); });
    controls();
}

void Window::configure(const QJsonObject &options) {
    for (auto it = fields.begin(); it != fields.end(); ++it) if (options.contains(it.key())) it.value()->setText(options[it.key()].toString());
    if (options.contains("cpu-budget")) budget->setValue(options["cpu-budget"].toDouble());
    if (options.contains("workers")) workers->setCurrentText(QString::number(options["workers"].toInt()));
}

void Window::connectBackend() {
    if (busy) return;
    if (process.state() != QProcess::NotRunning) {
        send({{"action","quit"}}); process.waitForFinished(3000);
        if (process.state() != QProcess::NotRunning) { error->setText("Previous signer is still closing. Wait before reconnecting."); return; }
    }
    current = {}; buffer.clear(); error->clear();
    verified = false; connecting = true; connectionFailed = false; inspected = false;
    QStringList args = {"serve"};
    for (const auto &key : {"manifest","read","submit","native","state-dir"}) args << "--" + QString(key) << fields[key]->text();
    if (!fields["wallet"]->text().isEmpty()) args << "--wallet" << fields["wallet"]->text();
    process.setProgram(backend); process.setArguments(args); process.start(); busy = true; controls();
    if (process.waitForStarted(3000)) {
        send({{"action","connect"},{"password", fields["password"]->text()}}); fields["password"]->clear(); status->setText("Verifying chain, bytecode and proxy wiring…");
    }
}

void Window::send(const QJsonObject &command) {
    if (process.state() == QProcess::Running) process.write(QJsonDocument(command).toJson(QJsonDocument::Compact) + '\n');
}

void Window::consume() {
    buffer += process.readAllStandardOutput();
    if (buffer.size() > 2 * 1024 * 1024) { error->setText("Backend response exceeded the allowed size."); process.kill(); return; }
    for (;;) {
        auto newline = buffer.indexOf('\n'); if (newline < 0) break;
        auto line = buffer.left(newline); buffer.remove(0, newline + 1);
        QJsonParseError parse; auto doc = QJsonDocument::fromJson(line, &parse);
        if (parse.error != QJsonParseError::NoError || !doc.isObject()) { error->setText("Invalid backend response."); continue; }
        update(doc.object()); emit backendEvent(doc.object());
    }
}

void Window::update(const QJsonObject &event) {
    auto type = event["event"].toString();
    if (type == "error") {
        error->setText(event["error"].toString());
        if (connecting) connectionFailed = true;
    }
    if (type == "command-complete") {
        if (connecting && event["action"].toString() == "connect") {
            verified = !connectionFailed && inspected && current.value("status").toString() == "connected";
            connecting = false;
            if (!verified) { current = QJsonObject(); status->setText("Disconnected"); }
        }
        busy = false;
        controls();
    }
    if (type == "state") {
        current = event["data"].toObject();
        const auto state = current["status"].toString();
        status->setText(state == "paused-resource"
            ? "Paused while this computer is busy. Mining resumes automatically."
            : state);
        metrics->setText(QString("%1 H/s · %2 hashes · %3 accepted · %4 rejected · %5 ZORU claimed")
            .arg(current["hashesPerSecond"].toDouble(), 0, 'f', 0).arg(current["hashes"].toDouble(), 0, 'f', 0)
            .arg(current["accepted"].toInt()).arg(current["rejected"].toInt()).arg(amount(current["rewardPaidBaseUnits"].toString("0"))));
        const auto events = current["events"].toArray(); proofs->setRowCount(events.size());
        for (int i = 0; i < events.size(); ++i) {
            auto proof = events[i].toObject();
            QStringList row = {QString::number(proof["epoch"].toInt()), proof["generation"].toVariant().toString(), proof["hash"].toString(), QString::number(proof["gasUsed"].toInt())};
            for (int j = 0; j < row.size(); ++j) proofs->setItem(i, j, new QTableWidgetItem(row[j]));
        }
        controls();
    }
    if (type == "inspection") {
        if (connecting) inspected = true;
        auto data = event["data"].toObject(); auto contracts = data["contracts"].toObject(); QStringList addresses;
        for (auto it = contracts.begin(); it != contracts.end(); ++it) addresses << it.key() + ": " + it.value().toString();
        deployment->setText(addresses.join("\n"));
        const auto reward = [&data](const char *key) {
            return data.contains(key) && data[key].isString()
                ? amount(data[key].toString()) + " ZORU" : QString("—");
        };
        const auto token = data.contains("tokenBalanceBaseUnits")
            ? reward("tokenBalanceBaseUnits") : QString("—");
        const auto native = data.contains("nativeBalanceWei")
            ? data["nativeBalanceWei"].toString() : QString("—");
        balance->setText("Address: " + data["address"].toString("read only")
            + " · ZORU balance: " + token + " · VEX balance (wei): " + native);
        myPending->setText(reward("myPendingBaseUnits"));
        myUnclaimed->setText(reward("myUnclaimedBaseUnits"));
        myClaimed->setText(reward("myClaimedBaseUnits"));
        if (data.contains("myRewardsHistoryComplete")) {
            myHistoryNote->setText(data["myRewardsHistoryComplete"].toBool()
                ? "From synced local proof history · Refresh to update"
                : "Partial proof history — reward estimates may be incomplete");
        } else {
            myHistoryNote->setText("Connect a wallet to view its proof rewards");
        }
        globalAllocated->setText(reward("allocatedSupplyBaseUnits"));
        globalPending->setText(reward("globalPendingBaseUnits"));
        globalUnclaimed->setText(reward("globalUnclaimedBaseUnits"));
        globalClaimed->setText(reward("globalClaimedBaseUnits"));
        globalMinted->setText(reward("totalSupplyBaseUnits"));
        if (data.contains("miningLaunched") && !data["miningLaunched"].toBool()) {
            networkInfo->setText(QString("Mining has not started · launch UNIX %1 · %2 accepted proofs")
                .arg(data["launchTime"].toString(), data["totalAcceptedProofs"].toString()));
        } else if (data["currentEpoch"].isString()) {
            networkInfo->setText(QString("Epoch %1 · %2 accepted proofs · Mining %3 · Claims %4")
                .arg(data["currentEpoch"].toString(), data["totalAcceptedProofs"].toString(),
                    data["admissionPaused"].toBool() ? "paused" : "active",
                    data["claimsPaused"].toBool() ? "paused" : "active"));
        } else {
            networkInfo->setText("Global network metrics unavailable");
        }
    }
}

void Window::controls() {
    const bool connected = verified && !current.isEmpty() && process.state() == QProcess::Running;
    const bool signer = !current.value("address").toString().isEmpty();
    connectButton->setEnabled(!busy); mine->setEnabled(connected && signer && !busy); claim->setEnabled(connected && signer && !busy); refresh->setEnabled(connected && !busy);
    stop->setEnabled(connected && busy); pause->setEnabled(connected && busy);
}

void Window::closeEvent(QCloseEvent *event) {
    shutdownBackend();
    event->accept();
}

Window::~Window() {
    shutdownBackend();
}

void Window::shutdownBackend() {
    // Process callbacks must stop before the window's member state is destroyed.
    process.disconnect(this);
    if (process.state() == QProcess::NotRunning) return;
    send({{"action","quit"}});
    if (!process.waitForFinished(5000) && process.state() != QProcess::NotRunning) { process.kill(); process.waitForFinished(2000); }
}
