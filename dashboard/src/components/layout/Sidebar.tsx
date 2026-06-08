import React from 'react';
import { useUiStore } from '../../store/uiStore';
import type { PageId } from '../../store/uiStore';
import { useFeedStore } from '../../store/feedStore';
import {
  Pulse,
  Stack,
  Network,
  Radio,
  Rss,
  Gear,
  CaretLeft,
  CaretRight,
  TrendUp,
} from '@phosphor-icons/react';

export const Sidebar: React.FC = () => {
  const { activePage, setActivePage, sidebarCollapsed, toggleSidebar } = useUiStore();
  const { health } = useFeedStore();

  const navItems = [
    { id: 'feed' as PageId, label: 'Live Feed', icon: Pulse },
    { id: 'events' as PageId, label: 'Events', icon: TrendUp },
    { id: 'narratives' as PageId, label: 'Narratives', icon: Stack },
    { id: 'graph' as PageId, label: 'Graph Explorer', icon: Network },
    { id: 'broadcasts' as PageId, label: 'Broadcasts', icon: Radio },
    { id: 'sources' as PageId, label: 'Sources', icon: Rss },
    { id: 'settings' as PageId, label: 'Settings', icon: Gear },
  ];

  return (
    <aside className={`sidebar ${sidebarCollapsed ? 'collapsed' : ''}`}>
      <div className="sidebar-header">
        <div className="logo-container">
          <span className="logo-text">objective</span>
          <span className="logo-version">v0.5</span>
        </div>
        <button onClick={toggleSidebar} className="collapse-btn">
          {sidebarCollapsed ? <CaretRight size={18} /> : <CaretLeft size={18} />}
        </button>
      </div>

      <nav className="sidebar-nav">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = activePage === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setActivePage(item.id)}
              className={`nav-item ${isActive ? 'active' : ''}`}
              title={sidebarCollapsed ? item.label : undefined}
            >
              <Icon className="nav-icon" size={20} />
              <span className="nav-label">{item.label}</span>
              {isActive && <div className="nav-indicator"></div>}
            </button>
          );
        })}
      </nav>

      <div className="sidebar-footer">
        <div className="status-indicator">
          <div className={`status-dot ${health?.status === 'healthy' ? 'healthy' : 'degraded'}`}></div>
          <span className="status-text">
            {sidebarCollapsed ? '' : health?.status === 'healthy' ? 'CONNECTED' : 'STANDBY'}
          </span>
        </div>
      </div>

      <style>{`
        .sidebar {
          background: var(--bg-secondary);
          border-right: 1px solid var(--border-color);
          display: flex;
          flex-direction: column;
          height: 100vh;
          transition: width var(--transition-normal);
          width: 260px;
          position: sticky;
          top: 0;
          z-index: 50;
        }

        .sidebar.collapsed {
          width: 80px;
        }

        @media (max-width: 768px) {
          .sidebar {
            width: 100%;
            height: auto;
            position: relative;
          }
          .sidebar.collapsed {
            width: 100%;
          }
          .sidebar-header {
            padding: 0.75rem 1rem !important;
          }
          .sidebar-nav {
            flex-direction: row !important;
            padding: 0.5rem !important;
            overflow-x: auto;
          }
          .nav-item {
            padding: 0.5rem 0.75rem !important;
            margin: 0 0.25rem !important;
          }
          .nav-label {
            display: none !important;
          }
          .collapse-btn, .sidebar-footer {
            display: none !important;
          }
        }

        .sidebar-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 1.25rem 1rem;
          border-bottom: 1px solid var(--border-color);
        }

        .logo-container {
          display: flex;
          align-items: center;
          gap: 0.5rem;
        }

        .logo-text {
          font-family: var(--font-mono);
          font-weight: 700;
          font-size: 1.1rem;
          letter-spacing: 0.05em;
          color: var(--text-primary);
          text-transform: lowercase;
        }

        .logo-version {
          font-size: 0.6rem;
          background: var(--bg-tertiary);
          color: var(--text-muted);
          padding: 0.1rem 0.35rem;
          border-radius: var(--radius-sm);
          font-weight: 500;
        }

        .collapsed .logo-text, .collapsed .logo-version {
          display: none;
        }

        .collapse-btn {
          color: var(--text-secondary);
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 0.35rem;
          border-radius: var(--radius-sm);
          border: 1px solid var(--border-color);
          background: var(--bg-tertiary);
          transition: background var(--transition-fast), color var(--transition-fast);
        }

        .collapse-btn:hover {
          background: var(--bg-elevated);
          color: var(--text-primary);
        }

        .sidebar-nav {
          display: flex;
          flex-direction: column;
          padding: 0.75rem 0.5rem;
          gap: 0.15rem;
          flex-grow: 1;
        }

        .nav-item {
          display: flex;
          align-items: center;
          gap: 0.75rem;
          padding: 0.6rem 0.75rem;
          color: var(--text-secondary);
          border-radius: var(--radius-sm);
          transition: background var(--transition-fast), color var(--transition-fast);
          position: relative;
          text-align: left;
          font-size: 0.8125rem;
        }

        .nav-item:hover {
          background: rgba(255, 255, 255, 0.04);
          color: var(--text-primary);
        }

        .nav-item.active {
          background: rgba(0, 122, 255, 0.08);
          color: var(--text-primary);
          font-weight: 500;
        }

        .nav-item.active .nav-icon {
          color: var(--accent-blue);
        }

        .nav-label {
          white-space: nowrap;
        }

        .collapsed .nav-label {
          display: none;
        }

        .nav-indicator {
          position: absolute;
          left: 0;
          top: 20%;
          height: 60%;
          width: 2px;
          background: var(--accent-blue);
          border-radius: 0 2px 2px 0;
        }

        .sidebar-footer {
          padding: 0.75rem 1rem;
          border-top: 1px solid var(--border-color);
        }

        .status-indicator {
          display: flex;
          align-items: center;
          gap: 0.5rem;
        }

        .status-text {
          font-size: 0.65rem;
          font-weight: 600;
          letter-spacing: 0.06em;
          color: var(--text-muted);
        }
      `}</style>
    </aside>
  );
};
