import { useEffect } from 'react';
import { useUiStore } from './store/uiStore';
import { useFeedStore } from './store/feedStore';
import { useWebSocket } from './hooks/useWebSocket';

// Layout Components
import { Sidebar } from './components/layout/Sidebar';
import { Header } from './components/layout/Header';
import { Footer } from './components/layout/Footer';
import { DetailDrawer } from './components/common/DetailDrawer';

// Pages
import { FeedPage } from './pages/FeedPage';
import { EventsPage } from './pages/EventsPage';
import { NarrativesPage } from './pages/NarrativesPage';
import { GraphPage } from './pages/GraphPage';
import { BroadcastsPage } from './pages/BroadcastsPage';
import { SourcesPage } from './pages/SourcesPage';
import { SettingsPage } from './pages/SettingsPage';

function App() {
  const { activePage, notifications, removeNotification } = useUiStore();
  const { fetchData } = useFeedStore();
  
  // Establish real-time websocket listener
  useWebSocket();

  // Load backend data on mount
  useEffect(() => {
    fetchData();
    
    // Poll stats & metrics every 15 seconds to keep dashboard fresh
    const interval = setInterval(() => {
      fetchData();
    }, 15000);

    return () => clearInterval(interval);
  }, [fetchData]);

  const renderActivePage = () => {
    switch (activePage) {
      case 'feed':
        return <FeedPage />;
      case 'events':
        return <EventsPage />;
      case 'narratives':
        return <NarrativesPage />;
      case 'graph':
        return <GraphPage />;
      case 'broadcasts':
        return <BroadcastsPage />;
      case 'sources':
        return <SourcesPage />;
      case 'settings':
        return <SettingsPage />;
      default:
        return <FeedPage />;
    }
  };

  return (
    <div className="app-container">
      {/* Navigation sidebar */}
      <Sidebar />

      {/* Main content frame */}
      <main className="main-content">
        <Header />

        <div className="content-body">
          {renderActivePage()}
        </div>

        <Footer />
      </main>

      {/* Slide-over details pane */}
      <DetailDrawer />

      {/* Notification Toast System */}
      <div className="toast-container">
        {notifications.map((toast) => (
          <div
            key={toast.id}
            onClick={() => removeNotification(toast.id)}
            className={`toast toast-${toast.type} glass-card animate-fade-in`}
          >
            <span>{toast.message}</span>
          </div>
        ))}
      </div>

      <style>{`
        /* Global Toast Overlays */
        .toast-container {
          position: fixed;
          bottom: 2rem;
          right: 2rem;
          display: flex;
          flex-direction: column;
          gap: 0.5rem;
          z-index: 1000;
          pointer-events: none;
        }

        .toast {
          pointer-events: auto;
          cursor: pointer;
          padding: 0.75rem 1.25rem !important;
          font-size: 0.85rem;
          font-weight: 500;
          min-width: 250px;
          max-width: 400px;
          border-left: 4px solid var(--accent-primary) !important;
          animation: slideUp 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards;
          text-align: left;
        }

        .toast-success {
          border-left-color: var(--color-success) !important;
        }

        .toast-warning {
          border-left-color: var(--color-warning) !important;
        }

        .toast-error {
          border-left-color: var(--color-danger) !important;
        }

        @keyframes slideUp {
          from {
            transform: translateY(20px);
            opacity: 0;
          }
          to {
            transform: translateY(0);
            opacity: 1;
          }
        }
      `}</style>
    </div>
  );
}

export default App;
