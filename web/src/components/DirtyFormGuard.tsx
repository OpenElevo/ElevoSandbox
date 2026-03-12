import { useEffect, useCallback } from 'react';
import { Modal } from 'antd';

interface DirtyFormGuardProps {
  dirty: boolean;
  message?: string;
}

export default function DirtyFormGuard({ dirty, message = '有未保存的内容，确认放弃？' }: DirtyFormGuardProps) {
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
        title: '未保存的更改',
        content: message,
        okText: '放弃',
        cancelText: '继续编辑',
        okButtonProps: { danger: true },
        autoFocusButton: 'cancel',
        onCancel: () => {
          // Push current state back to prevent navigation
          window.history.pushState(null, '', window.location.href);
        },
      });
    };
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, [dirty, message]);

  return null;
}
