#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QtTest>

namespace {
QStringList warnings;
QtMessageHandler previousHandler = nullptr;
void capture(QtMsgType type, const QMessageLogContext &context, const QString &message) {
    if (type == QtWarningMsg || type == QtCriticalMsg)
        warnings.append(message);
    if (previousHandler)
        previousHandler(type, context, message);
}
} // namespace
class UpdateDialogTest final : public QObject {
    Q_OBJECT
    QQmlApplicationEngine engine;
    QQuickWindow *window = nullptr;
    QObject *updates = nullptr, *dialog = nullptr, *notes = nullptr;
  private slots:
    void initTestCase() {
        QQuickStyle::setStyle("Basic");
        previousHandler = qInstallMessageHandler(capture);
        const QString fixture = QString(R"QML(
import QtQuick
import QtQuick.Controls
import "%1"
ApplicationWindow {
    visible: true; width: 1000; height: 700
    QtObject {
        id: updateState; objectName: "testUpdates"
        property string state: "Available"
        property string message: "A newer version is available."
        property string notes: ""
        property string downloadFolder: "Downloads/ZORU Miner"
        property string downloadFile: ""
        property real progress: 0
        property bool busy: false
        property bool installing: false
        property bool canDownload: true
        property bool canInstall: false
        property bool automaticChecks: true
        function check() {} function download() {} function cancelDownload() {} function openDownloadFolder() {}
    }
    QtObject {
        id: appState
        property string version: "9.0.0"
        property bool busy: false
        property bool mining: true
        property var updates: updateState
        function installUpdate() {}
    }
    UpdateDialog { state: appState }
})QML")
                                    .arg(QStringLiteral("qrc:/Zoru/app/qml/components"));
        engine.loadData(fixture.toUtf8());
        QVERIFY(!engine.rootObjects().isEmpty());
        window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        QVERIFY(window);
        QVERIFY(QTest::qWaitForWindowExposed(window));
        updates = window->findChild<QObject *>("testUpdates");
        dialog = window->findChild<QObject *>("softwareUpdateDialog");
        QVERIFY(updates);
        QVERIFY(dialog);
        notes = window->findChild<QObject *>("releaseNotes");
        QVERIFY(notes);
    }
    void release_notes_and_small_windows_do_not_create_layout_loops_data() {
        QTest::addColumn<int>("height");
        QTest::addColumn<QString>("state");
        for (int height : {700, 900})
            for (const auto &state : QStringList{"Available", "UpToDate", "Ready"}) {
                const auto name = QString::number(height) + "-" + state;
                QTest::newRow(qPrintable(name)) << height << state;
            }
    }
    void release_notes_and_small_windows_do_not_create_layout_loops() {
        QFETCH(int, height);
        QFETCH(QString, state);
        warnings.clear();
        QMetaObject::invokeMethod(dialog, "close");
        QTRY_VERIFY(!dialog->property("opened").toBool());
        window->setHeight(height);
        updates->setProperty("state", state);
        updates->setProperty("canDownload", state == "Available");
        updates->setProperty(
            "notes",
            QString(
                "# ZORU Miner\n\n- Handle interrupted update checks cleanly and allow another "
                "attempt.\n- Keep release notes and update controls usable on smaller windows.\n- "
                "Read wallet setup, mining instructions, the Keccak-256 algorithm and Vexanium "
                "consensus explanation from Help.\n- Check, download and install signed updates "
                "inside the Debian/Ubuntu application.\n- Keep the Rust worker active when the "
                "window is minimized, hidden or covered by another application.\n"));
        QTest::qWait(100);
        QVERIFY(QMetaObject::invokeMethod(dialog, "open"));
        QTRY_VERIFY(dialog->property("opened").toBool());
        QTest::qWait(100);
        QVERIFY(notes->property("width").toDouble() > 300);
        QVERIFY(notes->property("width").toDouble() < dialog->property("width").toDouble());
        QVERIFY(dialog->property("height").toDouble() <= window->height() - 40);
        updates->setProperty("state", "Ready");
        updates->setProperty("canDownload", false);
        updates->setProperty("downloadFile", "Downloads/ZORU Miner/update.deb");
        QTest::qWait(100);
        QVERIFY(dialog->property("height").toDouble() <= window->height() - 40);
        updates->setProperty("notes", QString(8192, 'x'));
        QTest::qWait(100);
        QVERIFY(dialog->property("height").toDouble() <= window->height() - 40);
        QVERIFY2(warnings.isEmpty(), qPrintable(warnings.join('\n')));
    }
    void cleanupTestCase() { qInstallMessageHandler(previousHandler); }
};
QTEST_MAIN(UpdateDialogTest)
#include "UpdateDialogTest.moc"
