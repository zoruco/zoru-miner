#include "Window.h"
#include <QApplication>
#include <QCommandLineParser>
#include <QFile>
#include <QJsonDocument>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName("ZORU Mainnet Miner");
    QCoreApplication::setOrganizationName("ZORU");
    QCommandLineParser parser; parser.addHelpOption();
    parser.addOption({"backend", "ZORU CLI executable", "path", QCoreApplication::applicationDirPath() + "/zoru-miner-cli"});
    parser.addOption({"config", "Public connection options JSON (no passwords)", "path"}); parser.process(app);
    Window window(parser.value("backend"));
    if (parser.isSet("config")) { QFile file(parser.value("config")); if (file.open(QIODevice::ReadOnly)) window.configure(QJsonDocument::fromJson(file.readAll()).object()); }
    window.show(); return app.exec();
}
