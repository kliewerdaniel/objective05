import React from 'react';
import { useUiStore } from '../../store/uiStore';
import type { PageId } from '../../store/uiStore';
import { useFeedStore } from '../../store/feedStore';
import {
  Activity,
  Layers,
  Network,
  Radio,
  Rss,
  Settings,
  ChevronLeft,
  ChevronRight,
  TrendingUp,
} from 'lucide-react';

export const Sidebar: React.FC = () => {
  const { activePage, setActivePage, sidebarCollapsed, toggleSidebar } = useUiStore();
  const { health } = useFeedStore();

  const navItems = [
    { id: 'feed' as PageId, label: 'Live Feed', icon: Activity },
    { id: 'events' as PageId, label: 'Events', icon: TrendingUp },
    { id: 'narratives' as PageId, label: 'Narratives', icon: Layers },
    { id: 'graph' as PageId, label: 'Graph Explorer', icon: Network },
    { id: 'broadcasts' as PageId, label: 'Broadcasts', icon: Radio },
    { id: 'sources' as PageId, label: 'Sources', icon: Rss },
    { id: 'settings' as PageId, label: 'Settings', icon: Settings },
  ];

  return (
    <aside className={`sidebar ${sidebarCollapsed ? 'collapsed' : ''}`}>
      <div className="sidebar-header">
        <div className="logo-container">
          <div className="logo-glow"></div>
          <span className="logo-text">OBJECTIVE</span>
          <span className="logo-version">v0.5</span>
        </div>
        <button onClick={toggleSidebar} className="collapse-btn">
          {sidebarCollapsed ? <ChevronRight size={18} /> : <ChevronLeft size={18} />}
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
            padding: 1rem !important;
          }
          .sidebar-nav {
            flex-direction: row !important;
            padding: 0.5rem !important;
            overflow-x: auto;
          }
          .nav-item {
            padding: 0.5rem 1rem !important;
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
          padding: 1.5rem;
          border-bottom: 1px solid var(--border-color);
        }

        .logo-container {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          position: relative;
        }

        .logo-glow {
          position: absolute;
          width: 24px;
          height: 24px;
          border-radius: 50%;
          background: var(--accent-gradient);
          filter: blur(8px);
          opacity: 0.5;
        }

        .logo-text {
          font-family: var(--font-display);
          font-weight: 700;
          font-size: 1.25rem;
          letter-spacing: 0.05em;
          background: var(--accent-gradient);
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
        }

        .logo-version {
          font-size: 0.65rem;
          background: rgba(139, 92, 246, 0.15);
          color: var(--accent-primary);
          padding: 0.1rem 0.35rem;
          border-radius: 4px;
          font-weight: 600;
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
          border-radius: 6px;
          border: 1px solid var(--border-color);
          background: var(--bg-tertiary);
          transition: background var(--transition-fast), color var(--transition-fast);
        }

        .collapse-btn:hover {
          background: var(--bg-glass-hover);
          color: var(--text-primary);
        }

        .sidebar-nav {
          display: flex;
          flex-direction: column;
          padding: 1rem 0.75rem;
          gap: 0.25rem;
          flex-grow: 1;
        }

        .nav-item {
          display: flex;
          align-items: center;
          gap: 1rem;
          padding: 0.75rem 1rem;
          color: var(--text-secondary);
          border-radius: var(--radius-md);
          transition: background var(--transition-fast), color var(--transition-fast);
          position: relative;
          text-align: left;
        }

        .nav-item:hover {
          background: rgba(255, 255, 255, 0.03);
          color: var(--text-primary);
        }

        .nav-item.active {
          background: rgba(139, 92, 246, 0.08);
          color: var(--text-primary);
          font-weight: 500;
        }

        .nav-icon {
          flex-shrink: 0;
          color: inherit;
        }

        .nav-item.active .nav-icon {
          color: var(--accent-primary);
        }

        .nav-label {
          font-size: 0.9rem;
          white-space: nowrap;
          transition: opacity var(--transition-normal);
        }

        .collapsed .nav-label {
          display: none;
        }

        .nav-indicator {
          position: absolute;
          left: 0;
          top: 25%;
          height: 50%;
          width: 3px;
          background: var(--accent-primary);
          border-radius: 0 4px 4px 0;
          box-shadow: 0 0 8px var(--accent-primary);
        }

        .sidebar-footer {
          padding: 1rem 1.5rem;
          border-top: 1px solid var(--border-color);
        }

        .status-indicator {
          display: flex;
          align-items: center;
          gap: 0.75rem;
        }

        .status-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
        }

        .status-dot.healthy {
          background: var(--color-success);
          box-shadow: 0 0 8px var(--color-success);
        }

        .status-dot.degraded {
          background: var(--color-warning);
          box-shadow: 0 0 8px var(--color-warning);
        }

        .status-text {
          font-size: 0.7rem;
          font-weight: 700;
          letter-spacing: 0.08em;
          color: var(--text-muted);
        }
      `}</style>
    </aside>
  );
};
