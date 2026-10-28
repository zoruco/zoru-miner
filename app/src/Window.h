#pragma once
#include <QWidget>
#include <QProcess>
#include <QJsonObject>
#include <QMap>
#include <QLineEdit>
#include <QLabel>
#include <QPushButton>
#include <QSpinBox>
#include <QDoubleSpinBox>
#include <QComboBox>
#include <QTableWidget>

class Window final : public QWidget {
    Q_OBJECT
public:
    explicit Window(QString backend, QWidget *parent = nullptr);
    ~Window() override;
    void configure(const QJsonObject &options);
    QJsonObject state() const { return current; }
    QString lastError() const { return error->text(); }
    void send(const QJsonObject &command);
signals:
    void backendEvent(const QJsonObject &event);
protected:
    void closeEvent(QCloseEvent *event) override;
private:
    QString backend;
    QProcess process;
    QByteArray buffer;
    QMap<QString, QLineEdit*> fields;
    QLabel *status, *error, *metrics, *balance, *deployment;
    QLabel *myPending, *myUnclaimed, *myClaimed, *myHistoryNote;
    QLabel *globalAllocated, *globalPending, *globalUnclaimed, *globalClaimed, *globalMinted, *networkInfo;
    QPushButton *connectButton, *mine, *stop, *claim, *pause, *refresh;
    QComboBox *workers;
    QDoubleSpinBox *budget;
    QTableWidget *proofs;
    QJsonObject current;
    bool busy = false;
    bool verified = false;
    bool connecting = false;
    bool connectionFailed = false;
    bool inspected = false;
    void connectBackend();
    void consume();
    void update(const QJsonObject &event);
    void controls();
    void shutdownBackend();
};
