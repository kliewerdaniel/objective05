import React, { useEffect, useState } from 'react';
import { useFeedStore } from '../store/feedStore';
import { useUiStore } from '../store/uiStore';
import type { SourceDefinition, SourceType } from '../api/types';
import { Rss, Plus, CheckCircle, WarningCircle, ArrowClockwise, Trash, Play, Power } from '@phosphor-icons/react';

const SOURCE_TYPE_OPTIONS: { value: SourceType; label: string; needsUrl: boolean }[] = [
  { value: 'rss', label: 'RSS / Atom Feed', needsUrl: true },
  { value: 'reddit', label: 'Reddit Subreddit', needsUrl: true },
  { value: 'youtube', label: 'YouTube Channel', needsUrl: true },
  { value: 'hackernews', label: 'Hacker News API', needsUrl: false },
  { value: 'arxiv', label: 'ArXiv Repository', needsUrl: true },
  { value: 'web', label: 'Web Scraper', needsUrl: true },
  { value: 'podcast', label: 'Podcast RSS', needsUrl: true },
  { value: 'github', label: 'GitHub Repository', needsUrl: true },
  { value: 'githubreleases', label: 'GitHub Releases', needsUrl: true },
  { value: 'secedgar', label: 'SEC EDGAR Filer', needsUrl: false },
  { value: 'static', label: 'Static Text Fixture', needsUrl: true },
];

export const SourcesPage: React.FC = () => {
  const { registeredSources, registryAvailable, refreshRegisteredSources, addRegisteredSource, updateRegisteredSource, removeRegisteredSource, triggerRegisteredSource } = useFeedStore();
  const { addNotification } = useUiStore();
  
  const [showAddForm, setShowAddForm] = useState(false);
  const [newSourceName, setNewSourceName] = useState('');
  const [newSourceType, setNewSourceType] = useState<SourceType>('rss');
  const [newSourceUrl, setNewSourceUrl] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [acting, setActing] = useState<string | null>(null);

  useEffect(() => {
    refreshRegisteredSources();
  }, [refreshRegisteredSources]);

  const needsUrl = SOURCE_TYPE_OPTIONS.find((o) => o.value === newSourceType)?.needsUrl ?? true;

  const handleAddSource = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newSourceName) return;
    if (needsUrl && !newSourceUrl) {
      addNotification('A URL is required for this source type.', 'error');
      return;
    }
    const now = new Date().toISOString();
    const source: SourceDefinition = {
      name: newSourceName.trim(),
      source_type: newSourceType,
      url: needsUrl ? newSourceUrl.trim() : undefined,
      enabled: true,
      created_at: now,
      updated_at: now,
    };
    setSubmitting(true);
    try {
      await addRegisteredSource(source);
      addNotification(`Source "${source.name}" registered.`, 'success');
      setShowAddForm(false);
      setNewSourceName('');
      setNewSourceUrl('');
      setNewSourceType('rss');
    } catch (e) {
      addNotification(`Failed to register source: ${(e as Error).message}`, 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`Remove source "${name}"?`)) return;
    setActing(name);
    try {
      await removeRegisteredSource(name);
      addNotification(`Source "${name}" removed.`, 'success');
    } catch (e) {
      addNotification(`Failed to remove: ${(e as Error).message}`, 'error');
    } finally {
      setActing(null);
    }
  };

  const handleToggle = async (name: string, currentlyEnabled: boolean) => {
    setActing(name);
    try {
      await updateRegisteredSource(name, { enabled: !currentlyEnabled });
      addNotification(
        `Source "${name}" ${!currentlyEnabled ? 'enabled' : 'disabled'}.`,
        'success',
      );
    } catch (e) {
      addNotification(`Failed to update: ${(e as Error).message}`, 'error');
    } finally {
      setActing(null);
    }
  };

  const handleTrigger = async (name: string) => {
    setActing(name);
    try {
      const count = await triggerRegisteredSource(name);
      addNotification(`Polled "${name}" — ${count} document(s) ingested.`, 'success');
    } catch (e) {
      addNotification(`Poll failed: ${(e as Error).message}`, 'error');
    } finally {
      setActing(null);
    }
  };

  return (
    <div className="sources-container animate-fade-in">
      <div className="sources-header-bar">
        <div>
          <h2 className="heading-md">Registered Ingestion Sources ({registeredSources.length})</h2>
          {!registryAvailable && (
            <p className="text-muted" style={{ fontSize: '0.8rem', marginTop: '0.25rem' }}>
              Source registry is not available on this gateway.
            </p>
          )}
        </div>
        <div style={{ display: 'flex', gap: '0.5rem' }}>
          <button
            onClick={() => refreshRegisteredSources()}
            className="btn-secondary"
            disabled={!registryAvailable}
          >
            <ArrowClockwise size={14} />
            Refresh
          </button>
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            className="btn-primary"
            disabled={!registryAvailable}
          >
            <Plus size={16} />
            Add Source
          </button>
        </div>
      </div>

      {/* Add Source Form */}
      {showAddForm && registryAvailable && (
        <form onSubmit={handleAddSource} className="add-source-form animate-fade-in">
          <h3 className="heading-md mb-3">New Ingestion Adapter</h3>
          
          <div className="form-fields">
            <div className="form-group">
              <label>Source Name</label>
              <input
                type="text"
                placeholder="e.g. wired-security-feed"
                value={newSourceName}
                onChange={(e) => setNewSourceName(e.target.value)}
                className="form-input"
                required
              />
            </div>

            <div className="form-group">
              <label>Source Type</label>
              <select
                value={newSourceType}
                onChange={(e) => setNewSourceType(e.target.value as SourceType)}
                className="form-input"
              >
                {SOURCE_TYPE_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>

            {needsUrl && (
              <div className="form-group">
                <label>URL / Handle / Repo</label>
                <input
                  type="text"
                  placeholder="https://example.com/feed.xml"
                  value={newSourceUrl}
                  onChange={(e) => setNewSourceUrl(e.target.value)}
                  className="form-input"
                  required
                />
              </div>
            )}
          </div>

          <div className="form-actions mt-4">
            <button type="submit" className="btn-primary" disabled={submitting}>
              {submitting ? 'Saving…' : 'Save Adapter'}
            </button>
            <button type="button" onClick={() => setShowAddForm(false)} className="btn-secondary">
              Cancel
            </button>
          </div>
        </form>
      )}

      {/* Sources Grid */}
      <div className="sources-grid">
        {registeredSources.length === 0 && registryAvailable && (
          <div className="empty-state">
            No sources registered yet. Click <strong>Add Source</strong> to wire up your first adapter.
          </div>
        )}
        {registeredSources.map((s) => {
          const isActing = acting === s.name;
          return (
            <div key={s.name} className="source-card">
              <div className="s-card-header">
                <div className="s-card-title-group">
                  <Rss size={18} className="text-muted" />
                  <h3 className="s-name">{s.name.replace(/_/g, ' ')}</h3>
                </div>
                <div className="s-status">
                  {s.enabled ? (
                    <CheckCircle size={16} className="text-success" />
                  ) : (
                    <WarningCircle size={16} className="text-warning" />
                  )}
                  <span className="s-status-text">{s.enabled ? 'Enabled' : 'Disabled'}</span>
                </div>
              </div>

              <div className="s-card-details">
                <div className="s-detail-row">
                  <span className="s-detail-label">Type:</span>
                  <span className="badge badge-blue">{s.source_type}</span>
                </div>
                {s.url && (
                  <div className="s-detail-row">
                    <span className="s-detail-label">URL:</span>
                    <span className="s-detail-value s-url" title={s.url}>{s.url}</span>
                  </div>
                )}
                <div className="s-detail-row">
                  <span className="s-detail-label">Created:</span>
                  <span className="s-detail-value">
                    {new Date(s.created_at).toLocaleDateString()}
                  </span>
                </div>
              </div>

              <div className="s-card-actions">
                <button
                  className="btn-secondary s-action-btn"
                  onClick={() => handleTrigger(s.name)}
                  disabled={isActing || !s.enabled}
                  title={s.enabled ? 'Poll this source now' : 'Enable the source to poll'}
                >
                  <Play size={14} />
                </button>
                <button
                  className="btn-secondary s-action-btn"
                  onClick={() => handleToggle(s.name, s.enabled)}
                  disabled={isActing}
                  title={s.enabled ? 'Disable' : 'Enable'}
                >
                  <Power size={14} />
                </button>
                <button
                  className="btn-secondary s-action-btn"
                  onClick={() => handleDelete(s.name)}
                  disabled={isActing}
                  title="Delete source"
                >
                  <Trash size={14} className="text-danger" />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <style>{`
        .sources-container {
          display: flex;
          flex-direction: column;
          gap: 1.5rem;
        }

        .sources-header-bar {
          display: flex;
          justify-content: space-between;
          align-items: flex-start;
          gap: 1rem;
        }

        .add-source-form {
          text-align: left;
          display: flex;
          flex-direction: column;
          gap: 1rem;
        }

        .form-fields {
          display: grid;
          grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
          gap: 1.25rem;
        }

        .form-group {
          display: flex;
          flex-direction: column;
          gap: 0.5rem;
        }

        .form-group label {
          font-size: 0.8rem;
          font-weight: 700;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }

        .form-actions {
          display: flex;
          gap: 1rem;
        }

        .sources-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
          gap: 1.25rem;
        }

        .empty-state {
          grid-column: 1 / -1;
          padding: 2rem;
          text-align: center;
          color: var(--text-secondary);
        }

        .source-card {
          text-align: left;
          display: flex;
          flex-direction: column;
          gap: 1rem;
        }

        .s-card-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          border-bottom: 1px solid var(--border-color);
          padding-bottom: 0.75rem;
        }

        .s-card-title-group {
          display: flex;
          align-items: center;
          gap: 0.5rem;
        }

        .s-name {
          font-family: var(--font-mono);
          font-size: 1.1rem;
          font-weight: 600;
          color: var(--text-primary);
          text-transform: capitalize;
        }

        .s-status {
          display: flex;
          align-items: center;
          gap: 0.35rem;
        }

        .s-status-text {
          font-size: 0.75rem;
          font-weight: 700;
          text-transform: uppercase;
          color: var(--text-secondary);
        }

        .s-card-details {
          display: flex;
          flex-direction: column;
          gap: 0.5rem;
          flex-grow: 1;
        }

        .s-detail-row {
          display: flex;
          justify-content: space-between;
          font-size: 0.85rem;
          gap: 0.5rem;
        }

        .s-detail-label {
          color: var(--text-secondary);
          white-space: nowrap;
        }

        .s-detail-value {
          color: var(--text-primary);
          font-weight: 500;
          text-align: right;
          word-break: break-all;
        }

        .s-url {
          font-family: var(--font-mono, monospace);
          font-size: 0.75rem;
          max-width: 180px;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .s-card-actions {
          display: flex;
          justify-content: flex-end;
          gap: 0.5rem;
          border-top: 1px solid var(--border-color);
          padding-top: 0.75rem;
        }

        .s-action-btn {
          padding: 0.4rem !important;
          border-radius: var(--radius-sm);
        }
      `}</style>
    </div>
  );
};
