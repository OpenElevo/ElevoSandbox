import { useState, useCallback } from 'react';

interface PaginationState {
  page: number;
  pageSize: number;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  onChange: (page: number, pageSize: number) => void;
  reset: () => void;
}

export function usePagination(defaultPageSize = 20): PaginationState {
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(defaultPageSize);

  const onChange = useCallback((p: number, ps: number) => {
    setPage(p);
    setPageSize(ps);
  }, []);

  const reset = useCallback(() => {
    setPage(1);
  }, []);

  return { page, pageSize, setPage, setPageSize, onChange, reset };
}
