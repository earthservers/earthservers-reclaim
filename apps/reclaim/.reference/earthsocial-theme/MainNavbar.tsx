import React, { useCallback } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Bell, Users, LogOut, ChevronDown, Heart, MessageSquare, Edit3, X } from 'lucide-react';
import { useScale, scaleW, scaleH } from '../../hooks/useScale';
import resolveAvatar from '../../lib/resolveAvatar';
import { notificationEventBus } from '../../lib/notificationEventBus';

const TurtleWithProfile = ({ size = 100, profilePicture, onProfileClick }: { size?: number; profilePicture?: string; onProfileClick?: () => void }) => {
  // Profile picture bigger to fill cutout - using percentage for proper scaling
  const profileSizePercent = 42; // 42% of turtle size
  // Use proportional positioning based on percentage - moved up slightly (was 22%)
  const turtleOffsetPercent = 21; // 21% of turtle size

  return (
    <div style={{ position: 'relative', width: '100%', height: '100%' }}>
      {/* Turtle SVG from backend - lower position */}
      <img
        src="/api/assets/profile-logo"
        alt=""
        style={{
          width: '100%',
          height: '100%',
          position: 'absolute',
          top: `${turtleOffsetPercent}%`,
          left: 0,
          zIndex: 1,
          pointerEvents: 'none'
        }}
      />

      {/* Profile Picture in the circular cutout - clickable */}
      <Link
        to="/u"
        style={{
          position: 'absolute',
          width: `${profileSizePercent}%`,
          height: `${profileSizePercent}%`,
          borderRadius: '50%',
          overflow: 'hidden',
          top: `calc(50% + ${turtleOffsetPercent}%)`,
          left: '50%',
          transform: 'translate(-50%, -50%)',
          zIndex: 0,
          display: 'block',
          textDecoration: 'none'
        }}
        className="hover:opacity-80 transition-opacity"
      >
        {profilePicture ? (
          <img
            src={profilePicture}
            alt="Profile"
            style={{
              width: '100%',
              height: '100%',
              objectFit: 'cover'
            }}
          />
        ) : (
          <div style={{
            width: '100%',
            height: '100%',
            background: 'linear-gradient(135deg, #006064, #00838f)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: '#e0f7fa',
            fontSize: `${size * 0.2}px`,
            fontWeight: 'bold'
          }}>
            ?
          </div>
        )}
      </Link>
    </div>
  );
};

interface MainNavbarProps {
  currentPath: string;
  onLogout: () => void;
  currentUser: any;
  notifications: any[];
  unreadCount: number;
  theme: any;
  customization?: any; // Raw customization object for untransformed colors
  navbarBg?: string; // Calculated navbar background with opacity
  showNotifications: boolean;
  setShowNotifications: (show: boolean) => void;
  showProfileMenu: boolean;
  setShowProfileMenu: (show: boolean) => void;
  onNotificationClick: (notification: any) => void;
  getNotificationIcon: (type: string) => React.ReactNode;
  getNotificationMessage: (notification: any) => string;
  formatNotificationTime: (createdAt: string) => string;
  setNotifications?: (updater: (prev: any[]) => any[]) => void;
  setUnreadCount?: (updater: (prev: number) => number) => void;
}

const navItems = [
  { label: 'Home', path: '/feed' },
  { label: 'Friends', path: '/friends' },
  { label: 'Messages', path: '/messages' },
  { label: 'Community', path: '/community' },
  { label: 'Ads', path: '/ads' },
  { label: 'Marketplace', path: '/marketplace' }
];

export default function MainNavbar({
  currentPath,
  onLogout,
  currentUser,
  notifications,
  unreadCount,
  theme,
  customization,
  navbarBg,
  showNotifications,
  setShowNotifications,
  showProfileMenu,
  setShowProfileMenu,
  onNotificationClick,
  getNotificationIcon,
  getNotificationMessage,
  formatNotificationTime,
  setNotifications,
  setUnreadCount
}: MainNavbarProps) {
  const navigate = useNavigate();
  const scale = useScale();

  // Memoize event handlers to prevent recreation on every render
  const handleToggleNotifications = useCallback(() => {
    setShowNotifications(!showNotifications);
  }, [showNotifications, setShowNotifications]);

  const handleToggleProfileMenu = useCallback(() => {
    setShowProfileMenu(!showProfileMenu);
  }, [showProfileMenu, setShowProfileMenu]);

  const handleCloseNotifications = useCallback(() => {
    setShowNotifications(false);
  }, [setShowNotifications]);

  const handleCloseProfileMenu = useCallback(() => {
    setShowProfileMenu(false);
  }, [setShowProfileMenu]);

  const handleLogoutClick = useCallback(() => {
    setShowProfileMenu(false);
    onLogout();
  }, [setShowProfileMenu, onLogout]);

  const handleMarkAllRead = useCallback(async () => {
    try {
      await fetch('/api/notifications/read-all', {
        method: 'POST',
        credentials: 'include'
      });
      if (setUnreadCount) setUnreadCount(() => 0);
      if (setNotifications) setNotifications(prev => prev.map(n => ({ ...n, read: true })));

      // Emit event for cross-component sync
      if (currentUser?.id) {
        notificationEventBus.emitReadAll(currentUser.id);
      }
    } catch (error) {
    }
  }, [setUnreadCount, setNotifications, currentUser?.id]);

  const handleNotificationRead = useCallback(async (notificationId: string) => {
    try {
      await fetch(`/api/notifications/${notificationId}/read`, {
        method: 'POST',
        credentials: 'include'
      });
      if (setNotifications) {
        setNotifications(prev => prev.map(n => n.id === notificationId ? { ...n, read: true } : n));
      }
      if (setUnreadCount) {
        setUnreadCount(prev => Math.max(0, prev - 1));
      }

      // Emit event for cross-component sync
      notificationEventBus.emitRead(notificationId);
    } catch (error) {
    }
  }, [setNotifications, setUnreadCount]);

  const handleDeleteNotification = useCallback(async (notificationId: string, e: React.MouseEvent) => {
    e.stopPropagation(); // Prevent notification click
    try {
      await fetch(`/api/notifications/${notificationId}`, {
        method: 'DELETE',
        credentials: 'include'
      });
      if (setNotifications) {
        setNotifications(prev => {
          const notification = prev.find(n => n.id === notificationId);
          // If deleting an unread notification, decrease unread count
          if (notification && !notification.read && setUnreadCount) {
            setUnreadCount(count => Math.max(0, count - 1));
          }
          return prev.filter(n => n.id !== notificationId);
        });
      }

      // Emit event for cross-component sync
      notificationEventBus.emitDelete(notificationId);
    } catch (error) {
    }
  }, [setNotifications, setUnreadCount]);

  // Memoize navigation handler for nav items
  const handleNavItemClick = useCallback((path: string) => {
    navigate(path);
  }, [navigate]);

  // Create handler for notification clicks
  const handleNotificationItemClick = useCallback(async (notification: any) => {
    if (!notification.read) {
      await handleNotificationRead(notification.id);
    }
    onNotificationClick(notification);
  }, [handleNotificationRead, onNotificationClick]);

  return (
    <nav className="sticky top-0 z-50 w-full border-b" style={{
      backgroundColor: navbarBg || theme.navbarBg || 'rgba(6, 182, 212, 0.95)',
      borderColor: 'rgba(255, 255, 255, 0.15)',
      backdropFilter: 'blur(10px)'
    }}>
      <div className="w-full flex items-center justify-center" style={{
        paddingLeft: `${scaleW(32, scale.w)}px`,
        paddingRight: `${scaleW(32, scale.w)}px`,
        height: `${scaleH(80, scale.h)}px`
      }}>
        <div className="w-full flex items-center justify-between" style={{
          paddingLeft: `${scaleW(40, scale.w)}px`,
          paddingRight: `${scaleW(80, scale.w)}px`,
          gap: `${scaleW(32, scale.w)}px`
        }}>
          {/* Left Side - EarthSocial Title */}
          <div className="flex-shrink-0">
            <h1 className="font-bold" style={{
              color: theme.textColor,
              fontSize: `${scaleW(48, scale.w)}px`
            }}>
              Earth<span style={{ color: '#E91E63' }}>Social</span>
            </h1>
          </div>

          {/* Navigation Items */}
          <div className="flex items-center justify-between flex-1" style={{ gap: `${scaleW(24, scale.w)}px` }}>
            {navItems.map((item, index) => (
              <button
                key={index}
                onClick={() => handleNavItemClick(item.path)}
                className="flex-1 font-semibold rounded-lg border-2 transition-opacity duration-300 hover:opacity-90 whitespace-nowrap"
                style={{
                  paddingLeft: `${scaleW(14, scale.w)}px`,
                  paddingRight: `${scaleW(14, scale.w)}px`,
                  paddingTop: `${scaleH(7, scale.h)}px`,
                  paddingBottom: `${scaleH(7, scale.h)}px`,
                  fontSize: `${scaleW(28, scale.w)}px`,
                  backgroundColor: item.path === currentPath ? theme.accentColor : 'rgba(255, 255, 255, 0.1)',
                  color: theme.textColor,
                  borderColor: item.path === currentPath ? theme.accentColor : 'rgba(255, 255, 255, 0.2)'
                }}
              >
                {item.label}
              </button>
            ))}
          </div>

          {/* Notifications Bell */}
          <div className="relative flex-shrink-0" style={{ marginTop: `${scaleH(1.6, scale.h)}px`, marginRight: `${scaleW(34, scale.w)}px` }}>
            <button
              onClick={handleToggleNotifications}
              className="relative p-2 transition-all hover:opacity-80"
              style={{
                color: theme.textColor
              }}
            >
              <Bell size={Math.round(scaleW(32, scale.w))} />
              {unreadCount > 0 && (
                <span
                  className="absolute -top-1 -right-1 rounded-full text-xs font-bold px-1.5 py-0.5"
                  style={{
                    backgroundColor: '#ef4444',
                    color: 'white',
                    fontSize: `${scaleW(12, scale.w)}px`,
                    minWidth: `${scaleW(20, scale.w)}px`,
                    textAlign: 'center'
                  }}
                >
                  {unreadCount > 99 ? '99+' : unreadCount}
                </span>
              )}
            </button>

            {/* Notifications Dropdown */}
            {showNotifications && (
              <>
                <div
                  className="absolute top-full right-0 mt-2 rounded-lg overflow-hidden shadow-2xl backdrop-blur-sm"
                  style={{
                    backgroundColor: theme.cardBg.replace(/[\d.]+\)$/, '0.98)'),
                    borderColor: theme.accentColor + '40',
                    border: '1px solid',
                    width: `${scaleW(400, scale.w)}px`,
                    maxHeight: `${scaleH(500, scale.h)}px`,
                    zIndex: 1001
                  }}
                >
                  {/* Header */}
                  <div
                    className="flex items-center justify-center p-4 border-b relative"
                    style={{
                      borderColor: theme.accentColor + '40',
                      backgroundColor: theme.tabBarBg || 'rgba(30, 58, 95, 0.95)'
                    }}
                  >
                    <h3 className="font-bold text-lg" style={{ color: theme.textColor }}>
                      Notifications
                    </h3>
                    {unreadCount > 0 && (
                      <button
                        className="absolute right-4 text-sm hover:opacity-80 transition-opacity"
                        onClick={handleMarkAllRead}
                        style={{ color: theme.accentColor }}
                      >
                        Mark all read
                      </button>
                    )}
                  </div>

                  {/* Notifications List */}
                  <div className="overflow-y-auto" style={{ maxHeight: `${scaleH(440, scale.h)}px` }}>
                    {notifications.length === 0 ? (
                      <div className="flex flex-col items-center justify-center py-12" style={{ color: theme.textColor + '99' }}>
                        <Bell size={48} style={{ color: theme.textColor + '40' }} className="mb-3" />
                        <p className="text-sm">No notifications yet</p>
                      </div>
                    ) : (
                      notifications.map((notification) => {
                        const notifIcon = getNotificationIcon(notification.type);
                        const notifMessage = getNotificationMessage(notification);

                        return (
                          <div
                            key={notification.id}
                            onClick={() => handleNotificationItemClick(notification)}
                            className="p-4 border-b cursor-pointer transition-all hover:opacity-80"
                            style={{
                              backgroundColor: notification.read ? 'transparent' : theme.accentColor + '10',
                              borderColor: theme.accentColor + '20'
                            }}
                          >
                            <div className="flex items-start gap-3">
                              {/* Avatar or Icon */}
                              {notification.actorAvatar ? (
                                <img
                                  src={notification.actorAvatar}
                                  alt={notification.actorName || 'User'}
                                  className="w-10 h-10 rounded-full object-cover flex-shrink-0"
                                />
                              ) : (
                                <div
                                  className="w-10 h-10 rounded-full flex items-center justify-center flex-shrink-0"
                                  style={{ backgroundColor: theme.accentColor + '40' }}
                                >
                                  {notifIcon}
                                </div>
                              )}

                              {/* Content */}
                              <div className="flex-1 min-w-0">
                                <p className="text-sm" style={{ color: theme.textColor }}>
                                  {notifMessage}
                                </p>
                                <p className="text-xs mt-1" style={{ color: theme.textColor + '66' }}>
                                  {formatNotificationTime(notification.createdAt)}
                                </p>
                              </div>

                              {/* Delete button */}
                              <button
                                onClick={(e) => handleDeleteNotification(notification.id, e)}
                                className="p-1 rounded transition-all flex-shrink-0 hover:opacity-70"
                                style={{
                                  color: theme.textColor + '99'
                                }}
                                title="Delete notification"
                              >
                                <X size={16} />
                              </button>

                              {/* Unread indicator */}
                              {!notification.read && (
                                <div
                                  className="w-2 h-2 rounded-full flex-shrink-0"
                                  style={{ backgroundColor: theme.accentColor }}
                                />
                              )}
                            </div>
                          </div>
                        );
                      })
                    )}
                  </div>
                </div>

                {/* Backdrop */}
                <div
                  className="fixed inset-0"
                  style={{ zIndex: 1000 }}
                  onClick={handleCloseNotifications}
                />
              </>
            )}
          </div>

          {/* Right Side - Profile/Logo */}
          <div className="flex flex-col items-center flex-shrink-0" style={{
            overflow: 'visible',
            zIndex: 100,
            position: 'relative',
            height: `${scaleH(80, scale.h)}px`
          }}>
            {/* Turtle with clickable profile picture */}
            <div
              style={{
                display: 'block',
                lineHeight: 0,
                padding: 0,
                margin: 0,
                width: `${scaleW(68, scale.w)}px`,
                height: `${scaleH(87, scale.h)}px`,
                overflow: 'visible',
                position: 'absolute',
                top: `${scaleH(-24, scale.h)}px`,
                left: '50%',
                transform: 'translateX(-50%)',
                zIndex: 100
              }}
            >
              <TurtleWithProfile
                size={scaleW(68, scale.w)}
                profilePicture={resolveAvatar(currentUser?.profilePicture || currentUser?.avatar) || undefined}
              />
            </div>

            {/* Dropdown Toggle Button - Centered directly below turtle */}
            <button
              onClick={handleToggleProfileMenu}
              className="hover:opacity-80 transition-opacity"
              style={{
                background: 'transparent',
                border: 'none',
                cursor: 'pointer',
                padding: '0px',
                position: 'absolute',
                top: `${scaleH(63, scale.h)}px`,
                left: '50%',
                transform: 'translateX(-50%)',
                zIndex: 101
              }}
            >
              <ChevronDown size={scaleH(14, scale.h)} style={{ color: theme.textColor }} />
            </button>

            {/* Profile Dropdown Menu */}
            {showProfileMenu && (
              <>
                {/* Backdrop */}
                <div
                  className="fixed inset-0"
                  style={{ zIndex: 999 }}
                  onClick={handleCloseProfileMenu}
                />
                {/* Menu */}
                <div
                  className="fixed rounded-lg shadow-lg border"
                  style={{
                    background: theme.cardBg,
                    borderColor: theme.accentColor + '40',
                    minWidth: '200px',
                    zIndex: 1000,
                    top: `${scaleH(80, scale.h)}px`,
                    right: `${scaleW(48, scale.w)}px`
                  }}
                >
                  <Link
                    to="/u"
                    className="flex items-center gap-3 px-4 py-3 hover:opacity-80 transition-opacity border-b"
                    style={{ color: theme.textColor, borderColor: theme.accentColor + '20' }}
                    onClick={handleCloseProfileMenu}
                  >
                    <Users size={20} />
                    <span>My Profile</span>
                  </Link>
                  <button
                    onClick={handleLogoutClick}
                    className="w-full flex items-center gap-3 px-4 py-3 hover:opacity-80 transition-opacity"
                    style={{ color: theme.textColor }}
                  >
                    <LogOut size={20} />
                    <span>Sign Out</span>
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </nav>
  );
}
