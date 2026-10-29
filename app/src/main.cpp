#include "AppState.h"
#include <QAction>
#include <QApplication>
#include <QCommandLineParser>
#include <QDir>
#include <QIcon>
#include <QLockFile>
#include <QMenu>
#include <QMessageBox>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QScreen>
#include <QStandardPaths>
#include <QSystemTrayIcon>
#include <QWindow>
int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    app.setOrganizationName("ZORU");
    app.setApplicationName("ZORU Miner");
    app.setApplicationVersion(QStringLiteral(ZORU_APP_VERSION));
    QCommandLineParser commandLine;
    commandLine.addOption({"data-dir", "Directory for wallet and preferences.", "directory"});
    commandLine.addHelpOption();
    commandLine.addVersionOption();
    commandLine.process(app);
    app.setWindowIcon(QIcon(":/branding/app.png"));
    QQuickStyle::setStyle("Basic");
    const QString directory =
        commandLine.isSet("data-dir")
            ? QDir(commandLine.value("data-dir")).absolutePath()
            : QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
    QDir().mkpath(directory);
    QLockFile instance(QDir(directory).filePath("app-instance.lock"));
    if (!instance.tryLock(100)) {
        QMessageBox::information(nullptr, "ZORU Miner",
                                 "ZORU Miner is already open. Switch to the existing window.");
        return 0;
    }
    AppState state(directory);
    QQmlApplicationEngine engine;
    QVariantMap initial{{"state", QVariant::fromValue(&state)}};
    if (const auto *screen = app.primaryScreen()) {
        const QSize available = screen->availableGeometry().size();
        initial.insert("width", qMin(1586, available.width()));
        initial.insert("height", qMin(992, available.height()));
    }
    engine.setInitialProperties(initial);
    engine.load(QUrl(QStringLiteral("qrc:/Zoru/app/qml/Main.qml")));
    if (engine.rootObjects().isEmpty())
        return 1;
    // Hiding or minimizing only changes the window. The Rust child keeps its
    // session, counters and control pipe until the user stops or quits.
    QSystemTrayIcon tray(app.windowIcon());
    QMenu trayMenu;
    if (QSystemTrayIcon::isSystemTrayAvailable()) {
        auto *window = qobject_cast<QWindow *>(engine.rootObjects().first());
        const auto show = [window] {
            if (window) {
                window->showNormal();
                window->raise();
                window->requestActivate();
            }
        };
        auto *open = trayMenu.addAction("Open ZORU Miner");
        QObject::connect(open, &QAction::triggered, &app, show);
        auto *hide = trayMenu.addAction("Hide Window");
        QObject::connect(hide, &QAction::triggered, &app, [window] {
            if (window)
                window->hide();
        });
        trayMenu.addSeparator();
        auto *stop = trayMenu.addAction("Stop Mining");
        QObject::connect(stop, &QAction::triggered, &state, &AppState::stopMining);
        auto *quit = trayMenu.addAction("Quit");
        QObject::connect(quit, &QAction::triggered, &app, [&state] {
            state.stopMining();
            QCoreApplication::quit();
        });
        QObject::connect(&tray, &QSystemTrayIcon::activated, &app,
                         [show](QSystemTrayIcon::ActivationReason reason) {
                             if (reason == QSystemTrayIcon::Trigger ||
                                 reason == QSystemTrayIcon::DoubleClick)
                                 show();
                         });
        tray.setContextMenu(&trayMenu);
        tray.setToolTip("ZORU Miner");
        tray.show();
    }
    return app.exec();
}
