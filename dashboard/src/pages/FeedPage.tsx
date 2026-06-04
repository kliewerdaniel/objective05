import React, { useState } from 'react';
import { useFeedStore } from '../store/feedStore';
import { useUiStore } from '../store/uiStore';
import { Search, Calendar, User, FileText } from 'lucide-react';
import type { RawDocument } from '../api/types';

export const FeedPage: React.FC = () => {
  const { documents, extractions } = useFeedStore();
  const { openDetail } = useUiStore();
  const [searchTerm, setSearchTerm] = useState('');
  const [filterSource, setFilterSource] = useState('all');

  // Filter logic
  const filteredDocs = documents.filter((doc: RawDocument) => {
    const matchesSearch =
      doc.title?.toLowerCase().includes(searchTerm.toLowerCase()) ||
      doc.body.toLowerCase().includes(searchTerm.toLowerCase()) ||
      doc.source_id.toLowerCase().includes(searchTerm.toLowerCase());
    const matchesSource = filterSource === 'all' || doc.source_id === filterSource;
    return matchesSearch && matchesSource;
  });

  const getSourcesList = () => {
    const set = new Set(documents.map((d: RawDocument) => d.source_id));
    return Array.from(set);
  };

  const formatDate = (dateStr?: string) => {
    if (!dateStr) return 'Unknown date';
    const d = new Date(dateStr);
    return d.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  return (
    <div className="feed-container animate-fade-in">
      {/* Search & Filter Bar */}
      <div className="search-filter-bar glass-card">
        <div className="search-wrapper">
          <Search size={18} className="search-icon" />
          <input
            type="text"
            placeholder="Search raw documents, entities, claims..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="form-input search-input"
          />
        </div>
        
        <select
          value={filterSource}
          onChange={(e) => setFilterSource(e.target.value)}
          className="form-input source-select"
        >
          <option value="all">All Sources</option>
          {getSourcesList().map((src) => (
            <option key={src} value={src}>
              {src.replace('_', ' ')}
            </option>
          ))}
        </select>
      </div>

      {/* Document List */}
      <div className="doc-list-wrapper">
        {filteredDocs.length === 0 ? (
          <div className="empty-state glass-card">
            <FileText size={48} className="text-muted" />
            <p className="heading-md">No documents found</p>
            <p className="text-muted">Ingestion daemon is polling. Check source configs.</p>
          </div>
        ) : (
          <div className="doc-grid">
            {filteredDocs.map((doc) => {
              const ext = extractions.find((e) => e.document_id === doc.id);
              const entityCount = ext?.entities.length || 0;
              const claimCount = ext?.claims.length || 0;

              return (
                <article
                  key={doc.id}
                  onClick={() => openDetail('document', doc.id)}
                  className="doc-card glass-card"
                >
                  <div className="doc-card-header">
                    <span className="badge badge-purple">{doc.source_type}</span>
                    <span className="doc-source">{doc.source_id.replace('_', ' ')}</span>
                  </div>

                  <h3 className="doc-title">{doc.title || 'Untitled Document'}</h3>
                  
                  <p className="doc-preview">
                    {doc.body.length > 160 ? `${doc.body.substring(0, 160)}...` : doc.body}
                  </p>

                  <div className="doc-card-footer">
                    <div className="doc-meta">
                      {doc.author && (
                        <span className="meta-item">
                          <User size={12} />
                          {doc.author}
                        </span>
                      )}
                      <span className="meta-item">
                        <Calendar size={12} />
                        {formatDate(doc.published_at || doc.fetched_at)}
                      </span>
                    </div>

                    <div className="doc-stats">
                      {entityCount > 0 && (
                        <span className="badge badge-teal" title="Extracted Entities">
                          {entityCount} entities
                        </span>
                      )}
                      {claimCount > 0 && (
                        <span className="badge badge-info" title="Extracted Claims">
                          {claimCount} claims
                        </span>
                      )}
                    </div>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </div>

      <style>{`
        .feed-container {
          display: flex;
          flex-direction: column;
          gap: 1.5rem;
        }

        .search-filter-bar {
          display: flex;
          gap: 1rem;
          padding: 1rem !important;
          align-items: center;
        }

        .search-wrapper {
          position: relative;
          flex-grow: 1;
        }

        .search-icon {
          position: absolute;
          left: 1rem;
          top: 50%;
          transform: translateY(-50%);
          color: var(--text-muted);
        }

        .search-input {
          padding-left: 2.75rem !important;
        }

        .source-select {
          width: 200px !important;
          text-transform: capitalize;
        }

        @media (max-width: 640px) {
          .search-filter-bar {
            flex-direction: column;
          }
          .source-select {
            width: 100% !important;
          }
        }

        .doc-grid {
          display: grid;
          grid-template-columns: 1fr;
          gap: 1rem;
        }

        .doc-card {
          cursor: pointer;
          display: flex;
          flex-direction: column;
          gap: 0.75rem;
          text-align: left;
        }

        .doc-card-header {
          display: flex;
          align-items: center;
          gap: 0.75rem;
        }

        .doc-source {
          font-size: 0.8rem;
          font-weight: 600;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }

        .doc-title {
          font-family: var(--font-display);
          font-size: 1.2rem;
          font-weight: 600;
          color: var(--text-primary);
        }

        .doc-preview {
          font-size: 0.9rem;
          color: var(--text-secondary);
          line-height: 1.6;
        }

        .doc-card-footer {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-top: 0.5rem;
          border-top: 1px solid rgba(255, 255, 255, 0.04);
          padding-top: 0.75rem;
          flex-wrap: wrap;
          gap: 0.5rem;
        }

        .doc-meta {
          display: flex;
          gap: 1rem;
          font-size: 0.75rem;
          color: var(--text-muted);
        }

        .meta-item {
          display: flex;
          align-items: center;
          gap: 0.35rem;
        }

        .doc-stats {
          display: flex;
          gap: 0.5rem;
        }

        .empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          padding: 4rem 2rem !important;
          gap: 1rem;
          text-align: center;
        }
      `}</style>
    </div>
  );
};
