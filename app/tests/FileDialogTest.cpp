#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTemporaryDir>
#include <QtTest>

class FileDialogTest final : public QObject {
    Q_OBJECT
  private slots:
    void fallback_dialog_opens_for_backup_and_import_data() {
        QTest::addColumn<QString>("mode");
        QTest::newRow("backup") << QString("SaveFile");
        QTest::newRow("import") << QString("OpenFile");
    }
    void fallback_dialog_opens_for_backup_and_import() {
        QFETCH(QString, mode);
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        QQmlApplicationEngine engine;
        QStringList warnings;
        connect(&engine, &QQmlEngine::warnings, this, [&](const QList<QQmlError> &errors) {
            for (const auto &error : errors)
                warnings.append(error.toString());
        });
        const QString qml = QString(R"QML(
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
ApplicationWindow {
    visible: true; width: 1000; height: 700
    FileDialog {
        objectName: "picker"; fileMode: FileDialog.%1
        options: FileDialog.DontUseNativeDialog
        currentFolder: "%2"
        nameFilters: ["Encrypted wallet (*.json)"]
    }
})QML")
                                .arg(mode, QUrl::fromLocalFile(directory.path()).toString());
        engine.loadData(qml.toUtf8());
        QVERIFY(!engine.rootObjects().isEmpty());
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        QVERIFY(window);
        QVERIFY(QTest::qWaitForWindowExposed(window));
        auto *picker = window->findChild<QObject *>("picker");
        QVERIFY(picker);
        QVERIFY(QMetaObject::invokeMethod(picker, "open"));
        QTRY_VERIFY(picker->property("visible").toBool());
        QTest::qWait(150);
        QVERIFY2(warnings.isEmpty(), qPrintable(warnings.join('\n')));
        QVERIFY(QMetaObject::invokeMethod(picker, "close"));
        QTRY_VERIFY(!picker->property("visible").toBool());
    }
};
int main(int argc, char **argv) {
    QGuiApplication application(argc, argv);
    QQuickStyle::setStyle("Basic");
    FileDialogTest test;
    return QTest::qExec(&test, argc, argv);
}
#include "FileDialogTest.moc"
