#pragma once
#include <QByteArray>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QTimer>

class WalletService final : public QObject {
    Q_OBJECT
  public:
    explicit WalletService(QObject *parent = nullptr);
    ~WalletService() override;
    bool ready() const;
    QJsonObject status(const QString &directory) const;
    void run(const QString &action, const QString &directory, const QString &file,
             QByteArray password, QByteArray extra = {}, QByteArray next = {});
  signals:
    void finished(const QString &action, const QJsonObject &result);

  private:
    QString executable() const;
    QProcess m_process;
    QTimer m_timeout;
    QByteArray m_input, m_output;
    QString m_action;
};
