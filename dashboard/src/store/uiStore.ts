import { create } from 'zustand';

export type PageId = 'feed' | 'events' | 'narratives' | 'graph' | 'broadcasts' | 'sources' | 'settings';

interface UiState {
  activePage: PageId;
  setActivePage: (page: PageId) => void;
  
  // Sidebar collapsed state
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  
  // Detail slide-in panels
  activeDetailId: string | null;
  activeDetailType: 'event' | 'entity' | 'narrative' | 'document' | null;
  openDetail: (type: 'event' | 'entity' | 'narrative' | 'document', id: string) => void;
  closeDetail: () => void;
  
  // Notifications
  notifications: Array<{ id: string; message: string; type: 'info' | 'success' | 'warning' | 'error' }>;
  addNotification: (message: string, type?: 'info' | 'success' | 'warning' | 'error') => void;
  removeNotification: (id: string) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activePage: 'feed',
  setActivePage: (page) => set({ activePage: page, activeDetailId: null, activeDetailType: null }),
  
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  
  activeDetailId: null,
  activeDetailType: null,
  openDetail: (type, id) => set({ activeDetailType: type, activeDetailId: id }),
  closeDetail: () => set({ activeDetailType: null, activeDetailId: null }),
  
  notifications: [],
  addNotification: (message, type = 'info') => set((state) => {
    const id = Math.random().toString(36).substring(7);
    // Auto-remove after 4 seconds
    setTimeout(() => {
      set((s) => ({ notifications: s.notifications.filter((n) => n.id !== id) }));
    }, 4000);
    return { notifications: [...state.notifications, { id, message, type }] };
  }),
  removeNotification: (id) => set((state) => ({
    notifications: state.notifications.filter((n) => n.id !== id),
  })),
}));
