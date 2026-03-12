import { create } from 'zustand';

interface BreadcrumbState {
  names: Record<string, string>;
  setBreadcrumbName: (id: string, name: string) => void;
  clearBreadcrumbName: (id: string) => void;
}

export const useBreadcrumbStore = create<BreadcrumbState>((set) => ({
  names: {},
  setBreadcrumbName: (id: string, name: string) => {
    set((state) => ({ names: { ...state.names, [id]: name } }));
  },
  clearBreadcrumbName: (id: string) => {
    set((state) => {
      const next = { ...state.names };
      delete next[id];
      return { names: next };
    });
  },
}));
