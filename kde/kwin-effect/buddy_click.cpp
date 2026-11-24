// SPDX-License-Identifier: MIT
// Native KWin effect: report content-free mouse clicks to Buddy.
//
// KWin scripts cannot see button events. This effect calls the daemon's session-bus
// bridge with the button and a coarse screen region; it never reads window contents.

#include <effect/effect.h>
#include <effect/effecthandler.h>

#include <QDBusConnection>
#include <QDBusMessage>
#include <QPointF>
#include <QString>
#include <Qt>

namespace KWin
{

class BuddyClickEffect : public Effect
{
    Q_OBJECT

public:
    BuddyClickEffect()
    {
        effects->startMousePolling();
        connect(effects, &KWin::EffectsHandler::mouseChanged, this, &BuddyClickEffect::mouseChanged);
    }

    ~BuddyClickEffect() override
    {
        effects->stopMousePolling();
    }

private Q_SLOTS:
    void mouseChanged(const QPointF &pos, const QPointF &oldpos, Qt::MouseButtons buttons, Qt::MouseButtons oldbuttons,
                      Qt::KeyboardModifiers modifiers, Qt::KeyboardModifiers oldmodifiers)
    {
        Q_UNUSED(oldpos);
        Q_UNUSED(modifiers);
        Q_UNUSED(oldmodifiers);

        quint32 button = 0;
        if ((buttons & Qt::LeftButton) && !(oldbuttons & Qt::LeftButton))
            button = 1;
        else if ((buttons & Qt::MiddleButton) && !(oldbuttons & Qt::MiddleButton))
            button = 2;
        else if ((buttons & Qt::RightButton) && !(oldbuttons & Qt::RightButton))
            button = 3;
        if (!button)
            return;

        // A coarse region, not a permanent click coordinate history.
        const QString region = QStringLiteral("%1:%2").arg(qRound(pos.x() / 240.0)).arg(qRound(pos.y() / 180.0));
        QDBusMessage message = QDBusMessage::createMethodCall(QStringLiteral("local.buddy.Sense"),
                                                              QStringLiteral("/Sense"),
                                                              QStringLiteral("local.buddy.Sense"),
                                                              QStringLiteral("Click"));
        message << button << region;
        QDBusConnection::sessionBus().send(message);
    }
};

KWIN_EFFECT_FACTORY(BuddyClickEffect, "metadata.json")

} // namespace KWin

#include "buddy_click.moc"
