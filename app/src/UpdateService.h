#pragma once
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QObject>
#include <QPointer>
#include <QProcess>
#include <QSaveFile>
#include <QSettings>
#include <QTimer>
#include <memory>

class UpdateServiceTest;
class UpdateService final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString state READ state NOTIFY changed)
    Q_PROPERTY(QString message READ message NOTIFY changed)
    Q_PROPERTY(QString availableVersion READ availableVersion NOTIFY changed)
    Q_PROPERTY(QString notes READ notes NOTIFY changed)
    Q_PROPERTY(QString downloadFolder READ downloadFolder NOTIFY changed)
    Q_PROPERTY(QString downloadFile READ downloadFile NOTIFY changed)
    Q_PROPERTY(double progress READ progress NOTIFY changed)
    Q_PROPERTY(bool busy READ busy NOTIFY changed)
    Q_PROPERTY(bool installing READ installing NOTIFY changed)
    Q_PROPERTY(bool canDownload READ canDownload NOTIFY changed)
    Q_PROPERTY(bool canInstall READ canInstall NOTIFY changed)
    Q_PROPERTY(bool automaticChecks READ automaticChecks WRITE setAutomaticChecks NOTIFY changed)
  public:
    explicit UpdateService(QObject *parent = nullptr);
    ~UpdateService() override;
    void initialize(const QString &storage, bool networkEnabled);
    QString state() const { return m_state; }
    QString message() const { return m_message; }
    QString availableVersion() const { return m_version; }
    QString notes() const { return m_notes; }
    QString downloadFolder() const { return m_folder; }
    QString downloadFile() const { return m_download; }
    double progress() const {
        return m_total > 0 ? qBound(0.0, double(m_received) / double(m_total), 1.0) : 0;
    }
    bool busy() const { return m_reply || m_process.state() != QProcess::NotRunning; }
    bool installing() const {
        return m_state == "Preparing" || m_state == "Installing" || m_state == "Restarting";
    }
    bool canDownload() const {
        return m_releaseValid && !busy() && (m_state == "Available" || m_state == "Error");
    }
    bool canInstall() const;
    bool automaticChecks() const { return m_automatic; }
    void setAutomaticChecks(bool enabled);
    Q_INVOKABLE void check();
    Q_INVOKABLE void download();
    Q_INVOKABLE void cancelDownload();
    Q_INVOKABLE void openDownloadFolder();
    void install();
    bool prepareInstall();
    void cancelPreparedInstall();
  signals:
    void changed();
    void available(const QString &version);
    void restartReady();

  private:
    friend class UpdateServiceTest;
    void setState(const QString &state, const QString &message);
    void verify(const QString &operation);
    void processFinished(int code, QProcess::ExitStatus status);
    void fail(const QString &message);
    QNetworkRequest request(const QUrl &url) const;
    QNetworkAccessManager m_network;
    QPointer<QNetworkReply> m_reply;
    QProcess m_process;
    QTimer m_deadline, m_periodic;
    std::unique_ptr<QSettings> m_preferences;
    std::unique_ptr<QSaveFile> m_file;
    QJsonObject m_artifact;
    QByteArray m_metadata, m_output;
    QString m_state = "Idle", m_message = "Check for a newer version of ZORU Miner.";
    QString m_version, m_notes, m_storage, m_folder, m_download, m_helper, m_operation;
    QUrl m_feed;
    qint64 m_received = 0, m_total = 0;
    quint64 m_generation = 0;
    bool m_automatic = true, m_enabled = false, m_releaseValid = false;
};
