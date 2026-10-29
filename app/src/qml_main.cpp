#include <QApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QSplashScreen>
#include <QPainter>
#include <QLinearGradient>
#include <QTimer>
#include <QtGlobal>
#include "AppState.h"

int main(int argc,char *argv[]){
    QApplication app(argc,argv);
    app.setOrganizationName("ZORU");
    app.setApplicationName("ZORU Miner");
    app.setWindowIcon(QIcon(":/assets/zoru-panda.png"));

    // Show startup artwork only while the native interface is initializing.
    QPixmap canvas(620,350);
    canvas.fill(QColor("#0D1012"));
    {
        QPainter painter(&canvas);
        painter.setRenderHint(QPainter::Antialiasing,true);
        QLinearGradient gradient(QPointF(0,0),QPointF(620,350));
        gradient.setColorAt(0,QColor("#14191B"));
        gradient.setColorAt(1,QColor("#0A0C0E"));
        painter.fillRect(canvas.rect(),gradient);
        QPixmap mascot(":/assets/zoru-panda.png");
        painter.drawPixmap(QRect(242,46,136,136),mascot);
        painter.setPen(QColor("#F4F1ED"));
        QFont title=app.font();title.setPixelSize(29);title.setWeight(QFont::DemiBold);
        painter.setFont(title);
        painter.drawText(QRect(0,194,620,52),Qt::AlignCenter,QStringLiteral("ZORU Miner"));
        painter.setPen(QColor("#9AA39F"));
        QFont subtitle=app.font();subtitle.setPixelSize(13);
        painter.setFont(subtitle);
        painter.drawText(QRect(0,246,620,40),Qt::AlignCenter,QStringLiteral("Your mining workspace"));
    }
    QSplashScreen splash(canvas);
    splash.show();
    app.processEvents();

    QQmlApplicationEngine engine;
    AppState state;
    engine.rootContext()->setContextProperty("zoru",&state);
#if QT_VERSION >= QT_VERSION_CHECK(6,5,0)
    engine.loadFromModule("Zoru","Main");
#else
    engine.load(QUrl(QStringLiteral("qrc:/Zoru/app/qml/Main.qml")));
#endif
    if(engine.rootObjects().isEmpty())return 1;
    // Short, deterministic handover without an artificial loading animation.
    QTimer::singleShot(420,&splash,&QSplashScreen::close);
    return app.exec();
}
