import { useEffect, useCallback } from 'react';
import { Modal } from 'antd';

interface DirtyFormGuardProps {
  dirty: boolean;
  message?: string;
}

export default function DirtyFormGuard({ dirty, message = 'You have unsaved changes. Discard them?' }: DirtyFormGuardProps) {
  const handleBeforeUnload = useCallback((e: BeforeUnloadEvent) => {
    if (dirty) {
      e.preventDefault();
    }
  }, [dirty]);

  useEffect(() => {
    window.addEventListener('beforeunload', handleBeforeUnload);
    return () => window.removeEventListener('beforeunload', handleBeforeUnload);
  }, [handleBeforeUnload]);

  // Also guard react-router navigation
  useEffect(() => {
    if (!dirty) return;
    const handlePopState = () => {
      Modal.confirm({
        title: 'Unsaved Changes',
        content: message,
        okText: 'Discard',
        cancelText: 'Stay',
        okButtonProps: { danger: true },
        onCancel: () => {
          // Push current state back
          window.history.pushState(null, '', window.location.href);
        },
      });
    };
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, [dirty, message]);

  return null;
}
