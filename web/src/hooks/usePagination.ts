import { useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

interface PaginationState {
  page: number;
  pageSize: number;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  onChange: (page: number, pageSize: number) => void;
  reset: () => void;
}

export function usePagination(defaultPageSize = 20): PaginationState {
  const [searchParams, setSearchParams] = useSearchParams();

  const page = parseInt(searchParams.get('page') ?? '1', 10) || 1;
  const pageSize = parseInt(searchParams.get('page_size') ?? String(defaultPageSize), 10) || defaultPageSize;

  const updateParams = useCallback((newPage: number, newPageSize: number) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.set('page', String(newPage));
      next.set('page_size', String(newPageSize));
      return next;
    }, { replace: true });
  }, [setSearchParams]);

  const setPage = useCallback((p: number) => {
    updateParams(p, pageSize);
  }, [updateParams, pageSize]);

  const setPageSize = useCallback((size: number) => {
    updateParams(1, size);
  }, [updateParams]);

  const onChange = useCallback((p: number, ps: number) => {
    updateParams(p, ps);
  }, [updateParams]);

  const reset = useCallback(() => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.set('page', '1');
      next.set('page_size', String(defaultPageSize));
      return next;
    }, { replace: true });
  }, [setSearchParams, defaultPageSize]);

  return { page, pageSize, setPage, setPageSize, onChange, reset };
}
