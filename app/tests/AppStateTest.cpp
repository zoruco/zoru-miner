#include "AppState.h"
#include <QApplication>
#include <QFile>
#include <QJsonDocument>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QSaveFile>
#include <QSslSocket>
#include <QTemporaryDir>
#include <QtTest>

class AppStateTest final : public QObject {
    Q_OBJECT
    QJsonObject snapshot() const {
        return {{"chainId", 6736},
                {"blockNumber", "11100000"},
                {"blockHash", "0x" + QString(64, '1')},
                {"blockTimestamp", QString::number(QDateTime::currentSecsSinceEpoch())},
                {"currentEpoch", "150"},
                {"miningLaunched", true},
                {"rpcLatencyMilliseconds", 42},
                {"totalSupplyBaseUnits", "2100000000000000"},
                {"allocatedSupplyBaseUnits", "1234567890123456"},
                {"fundedSupplyBaseUnits", "1000000000"},
                {"paidSupplyBaseUnits", "500000000"},
                {"globalPendingBaseUnits", "1234566890123456"},
                {"globalUnclaimedBaseUnits", "500000000"},
                {"totalAcceptedProofs", "9007199254740993"},
                {"nextEmissionBudgetBaseUnits", "500000000"},
                {"halvingEpochs", "2100000"},
                {"halvingProgressEpochs", "100"},
                {"epochsUntilHalving", "2099900"},
                {"halvingEra", "0"},
                {"followingEraBudgetBaseUnits", "250000000"},
                {"capBaseUnits", "2100000000000000"},
                {"lifetimeMintedBaseUnits", "1000000000"},
                {"transactionFeeCapWei", "3300000000000000000"}};
    }
  private slots:
    void secure_download_transport_is_available() { QVERIFY(QSslSocket::supportsSsl()); }
    void global_contract_snapshot_preserves_financial_precision() {
        QTemporaryDir directory;
        AppState state(directory.path(), false);
        state.applyInspection(snapshot());
        QVERIFY(state.networkHealthy());
        QCOMPARE(state.globalStats().value("allocated").toString(), QString("12345678.90123456"));
        QCOMPARE(state.globalStats().value("totalProofs").toString(), QString("9007199254740993"));
        QCOMPARE(state.globalStats().value("transactionCap").toString(), QString("3.3"));
        QCOMPARE(state.halving().value("epochsUntilHalving").toString(), QString("2099900"));
        QVERIFY(!state.miningBlocker().isEmpty());
        auto invalid = snapshot();
        invalid["chainId"] = 1;
        state.applyInspection(invalid);
        QVERIFY(!state.networkHealthy());
    }
    void accepted_proofs_do_not_increment_claimed_rewards() {
        QTemporaryDir directory;
        AppState state(directory.path(), false);
        state.m_running = true;
        state.m_sessionAuthorized = true;
        state.applyMinerEvent({{"event", "state"},
                               {"data", QJsonObject{{"status", "mining"},
                                                    {"accepted", 50},
                                                    {"rejected", 0},
                                                    {"hashes", 1000},
                                                    {"validFound", 60}}}});
        QCOMPARE(state.sessionAccepted(), 50);
        QCOMPARE(state.confirmedProofs(), qulonglong(0));
        QCOMPARE(state.confirmedRewards(), QString("—"));
    }
    void framed_output_accepts_large_bursts_and_rejects_oversized_frames() {
        QTemporaryDir directory;
        AppState state(directory.path(), false);
        state.m_running = true;
        QByteArray bytes;
        for (int i = 0; i < 9000; ++i)
            bytes += QJsonDocument(QJsonObject{{"event", "state"},
                                               {"data", QJsonObject{{"status", "paused"},
                                                                    {"accepted", i},
                                                                    {"fees", QJsonObject{}}}}})
                         .toJson(QJsonDocument::Compact) +
                     '\n';
        QVERIFY(bytes.size() > 512 * 1024);
        state.consumeFrames(bytes, true);
        QVERIFY(bytes.isEmpty());
        QCOMPARE(state.sessionAccepted(), 8999);
        state.m_sessionAuthorized = true;
        bytes = QByteArray(1024 * 1024 + 1, 'x');
        state.consumeFrames(bytes, true);
        QVERIFY(bytes.isEmpty());
        QVERIFY(!state.m_sessionAuthorized);
    }
    void stop_revokes_a_password_waiting_for_backend_start() {
        QTemporaryDir directory;
        AppState state(directory.path(), false);
        state.m_pendingCredential = "disposable test password";
        state.m_startCredential = "disposable test password";
        state.m_sessionAuthorized = true;
        state.m_signAfterUnlock = true;
        state.stopMining();
        QVERIFY(state.m_pendingCredential.isEmpty());
        QVERIFY(state.m_startCredential.isEmpty());
        QVERIFY(!state.m_sessionAuthorized);
        QVERIFY(!state.m_signAfterUnlock);
    }
    void live_mining_uses_the_same_desktop_controls_and_contract_snapshot() {
        const QString profile = qEnvironmentVariable("ZORU_LIVE_UI_PROFILE");
        if (profile.isEmpty())
            QSKIP(
                "Requires an explicitly configured live profile and inherited password descriptor");
        const QString expected = qEnvironmentVariable("ZORU_LIVE_EXPECTED_ADDRESS");
        QVERIFY(!expected.isEmpty());
        bool valid = false;
        const int descriptor = qEnvironmentVariable("ZORU_LIVE_PASSWORD_FD").toInt(&valid);
        QVERIFY(valid);
        QFile input;
        QVERIFY(input.open(descriptor, QIODevice::ReadOnly));
        QByteArray password = input.readLine(4096);
        while (password.endsWith('\n') || password.endsWith('\r'))
            password.chop(1);
        QVERIFY(!password.isEmpty());
        AppState state(profile, true);
        QVERIFY(state.address().compare(expected, Qt::CaseInsensitive) == 0);
        QTRY_VERIFY_WITH_TIMEOUT(state.networkHealthy(), 180000);
        QVERIFY(!state.globalStats().value("totalProofs").toString().isEmpty());
        qInfo("Verified contract snapshot ready.");
        state.setStopAfterProof(true);
        state.setSessionBudget("1");
        state.setPage("Miner");
        QQmlApplicationEngine engine;
        engine.setInitialProperties({{"state", QVariant::fromValue(&state)}});
        engine.load(QUrl("qrc:/Zoru/app/qml/Main.qml"));
        QVERIFY(!engine.rootObjects().isEmpty());
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        QVERIFY(window);
        QVERIFY(QTest::qWaitForWindowExposed(window));
        const auto click = [window](const char *name) {
            auto *item = window->findChild<QQuickItem *>(name);
            if (!item || !item->isEnabled() || !item->isVisible())
                return false;
            QTest::mouseClick(
                window, Qt::LeftButton, Qt::NoModifier,
                item->mapToScene(QPointF(item->width() / 2, item->height() / 2)).toPoint());
            return true;
        };
        QVERIFY2(state.miningBlocker().isEmpty(), qPrintable(state.miningBlocker()));
        QVERIFY(click("startMiningButton"));
        auto *dialog = window->findChild<QObject *>("walletDialog");
        QVERIFY(dialog);
        QTRY_VERIFY(dialog->property("visible").toBool());
        auto *field = dialog->findChild<QObject *>("dialogPassword");
        auto *authorization = dialog->findChild<QObject *>("dialogAuthorization");
        QVERIFY(field);
        QVERIFY(authorization);
        field->setProperty("text", QString::fromUtf8(password));
        authorization->setProperty("checked", true);
        password.fill(0);
        password.clear();
        QVERIFY(click("dialogContinue"));
        QTRY_VERIFY_WITH_TIMEOUT(state.mining(), 30000);
        QTRY_VERIFY_WITH_TIMEOUT(state.sessionAccepted() == 1, 180000);
        QTRY_VERIFY_WITH_TIMEOUT(!state.mining(), 30000);
        qInfo("Single proof confirmed.");
        QCOMPARE(state.sessionRejected(), 0);
        QVERIFY(state.sessionFeesSpent() != "—");
        QVERIFY(state.sessionFeesSpent() != "0");
        state.startMonitor();
        QTRY_VERIFY_WITH_TIMEOUT(!state.proofs().isEmpty(), 180000);
        state.setStopAfterProof(false);
        state.setPage("Miner");
        const auto authorize = [&]() {
            if (!input.seek(0))
                return false;
            QByteArray secret = input.readLine(4096);
            while (secret.endsWith('\n') || secret.endsWith('\r'))
                secret.chop(1);
            field->setProperty("text", QString::fromUtf8(secret));
            authorization->setProperty("checked", true);
            secret.fill(0);
            secret.clear();
            return click("dialogContinue");
        };
        QVERIFY(click("startMiningButton"));
        QTRY_VERIFY(dialog->property("visible").toBool());
        QVERIFY(authorize());
        QTRY_VERIFY_WITH_TIMEOUT(state.activelyHashing(), 120000);
        QVERIFY(click("pauseResumeMiningButton"));
        QTRY_VERIFY_WITH_TIMEOUT(state.m_engineHolds.contains("manual"), 10000);
        QVERIFY(!state.activelyHashing());
        qInfo("Pause confirmed.");
        QVERIFY(click("pauseResumeMiningButton"));
        QTRY_VERIFY_WITH_TIMEOUT(state.activelyHashing(), 120000);
        QTRY_VERIFY_WITH_TIMEOUT(state.sessionAccepted() >= 8, 180000);
        qInfo("Continuous mining confirmed.");
        QVERIFY(click("stopMiningButton"));
        QTRY_VERIFY_WITH_TIMEOUT(!state.mining(), 30000);
        QCOMPARE(state.sessionRejected(), 0);
        QVERIFY(!state.signingAuthorized());
        const QString miningFees = state.sessionFeesSpent();
        qInfo("Mining stopped; preparing manual claims.");
        const QString claimedBefore = state.confirmedRewards();
        state.setSessionBudget("0.2");
        state.setPage("Rewards");
        QTest::qWait(100);
        QVERIFY(click("claimRewardsButton"));
        QTRY_VERIFY(dialog->property("visible").toBool());
        QVERIFY(authorize());
        QTRY_VERIFY_WITH_TIMEOUT(state.mining(), 30000);
        QTRY_VERIFY_WITH_TIMEOUT(!state.mining(), 180000);
        qInfo("Claim session ended.");
        state.startMonitor();
        QTRY_VERIFY_WITH_TIMEOUT(state.confirmedRewards() != claimedBefore, 180000);
        QVERIFY(!state.signingAuthorized());
        qInfo("Claimed rewards refreshed from chain history.");
        const auto file = QDir(profile).filePath("live-ui-result.json");
        QSaveFile result(file);
        QVERIFY(result.open(QIODevice::WriteOnly));
        result.setPermissions(QFile::ReadOwner | QFile::WriteOwner);
        result.write(
            QJsonDocument(QJsonObject{{"accepted", state.sessionAccepted()},
                                      {"rejected", state.sessionRejected()},
                                      {"miningFeesVex", miningFees},
                                      {"claimFeesVex", state.sessionFeesSpent()},
                                      {"claimedRewards", state.confirmedRewards()},
                                      {"block", state.height()},
                                      {"global", QJsonObject::fromVariantMap(state.globalStats())}})
                .toJson());
        QVERIFY(result.commit());
        QDir(profile).mkpath("screenshots");
        QVERIFY(window->grabWindow().save(QDir(profile).filePath("screenshots/miner.png")));
        for (const auto &page : QStringList{"Overview", "Rewards", "Proofs", "Settings"}) {
            state.setPage(page);
            QTest::qWait(150);
            QVERIFY(window->grabWindow().save(
                QDir(profile).filePath("screenshots/" + page.toLower() + ".png")));
        }
    }
    void every_page_loads_with_current_contract_data() {
        QTemporaryDir directory;
        AppState state(directory.path(), false);
        state.m_introductionComplete = true;
        state.applyInspection(snapshot());
        QQmlApplicationEngine engine;
        QStringList errors;
        connect(&engine, &QQmlEngine::warnings, this, [&](const QList<QQmlError> &warnings) {
            for (const auto &warning : warnings)
                if (warning.toString().contains("ReferenceError") ||
                    warning.toString().contains("TypeError") ||
                    warning.toString().contains("Binding loop"))
                    errors.append(warning.toString());
        });
        engine.setInitialProperties({{"state", QVariant::fromValue(&state)}});
        engine.load(QUrl("qrc:/Zoru/app/qml/Main.qml"));
        QVERIFY2(!engine.rootObjects().isEmpty(), qPrintable(errors.join('\n')));
        for (const auto &page : QStringList{"Overview", "Miner", "Rewards", "Wallet", "Proofs",
                                            "Nodes", "Security", "Settings", "Help"}) {
            state.setPage(page);
            QTest::qWait(50);
        }
        QVERIFY2(errors.isEmpty(), qPrintable(errors.join('\n')));
    }
};
int main(int argc, char **argv) {
    QApplication application(argc, argv);
    application.setApplicationName("ZORU Miner test");
    application.setApplicationVersion(QStringLiteral(ZORU_APP_VERSION));
    QQuickStyle::setStyle("Basic");
    AppStateTest test;
    return QTest::qExec(&test, argc, argv);
}
#include "AppStateTest.moc"
