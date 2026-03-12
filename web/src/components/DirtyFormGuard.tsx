import { useEffect, useCallback } from 'react';
import { useBlocker } from 'react-router-dom';
import { Modal } from 'antd';

interface DirtyFormGuardProps {
  dirty: boolean;
  message?: string;
}

/**
 * Prevents accidental navigation away from a form with unsaved changes.
 * Intercepts both browser-level events (tab close, refresh) and
 * React Router programmatic/link navigation via useBlocker.
 */
export default function DirtyFormGuard({ dirty, message = '有未保存的内容，确认放弃？' }: DirtyFormGuardProps) {
  // Block browser tab close / refresh
  const handleBeforeUnload = useCallback((e: BeforeUnloadEvent) => {
    if (dirty) {
      e.preventDefault();
    }
  }, [dirty]);

  useEffect(() => {
    window.addEventListener('beforeunload', handleBeforeUnload);
    return () => window.removeEventListener('beforeunload', handleBeforeUnload);
  }, [handleBeforeUnload]);

  // Block React Router navigation (navigate(), <Link>, back/forward)
  const blocker = useBlocker(dirty);

  useEffect(() => {
    if (blocker.state === 'blocked') {
      Modal.confirm({
        title: '未保存的更改',
        content: message,
        okText: '放弃',
        cancelText: '继续编辑',
        okButtonProps: { danger: true },
        autoFocusButton: 'cancel',
        onOk: () => blocker.proceed(),
        onCancel: () => blocker.reset(),
      });
    }
  }, [blocker, message]);

  return null;
}
