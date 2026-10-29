#include "Window.h"
#include <QApplication>
#include <QCommandLineParser>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName("ZORU Mainnet Miner");
    QCoreApplication::setOrganizationName("ZORU");
    QString binary = QCoreApplication::applicationDirPath() + "/zoru-miner-cli";
#ifdef Q_OS_WIN
    binary += ".exe";
#endif
    QCommandLineParser parser; parser.addHelpOption();
    parser.addOption({"backend", "ZORU CLI executable", "path", binary});
    parser.addOption({"config", "Public connection options JSON (no passwords)", "path"}); parser.process(app);
    Window window(parser.value("backend"));
    const QString manifest = QDir::cleanPath(QCoreApplication::applicationDirPath() + "/../deployment.json");
    QJsonObject defaults = {
        {"manifest", QFile::exists(manifest) ? manifest : QString()},
        {"read", "https://vexascan.com/rpc"},
        {"submit", "https://vexascan.com/rpc"},
        {"native", "https://vexascan.com:8443"}
    };
    window.configure(defaults);
    if (parser.isSet("config")) {
        QFile file(parser.value("config"));
        if (file.open(QIODevice::ReadOnly))
            window.configure(QJsonDocument::fromJson(file.readAll()).object());
    }
    window.show(); return app.exec();
}
