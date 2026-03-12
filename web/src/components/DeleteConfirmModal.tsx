import { useState } from 'react';
import { Modal, Input, Typography } from 'antd';

interface DeleteConfirmModalProps {
  open: boolean;
  title: string;
  entityName: string;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
  loading?: boolean;
}

export default function DeleteConfirmModal({ open, title, entityName, onConfirm, onCancel, loading }: DeleteConfirmModalProps) {
  const [inputValue, setInputValue] = useState('');
  const [error, setError] = useState('');

  const handleOk = async () => {
    if (inputValue !== entityName) {
      setError('Name does not match');
      return;
    }
    setError('');
    await onConfirm();
    setInputValue('');
  };

  const handleCancel = () => {
    setInputValue('');
    setError('');
    onCancel();
  };

  return (
    <Modal
      title={title}
      open={open}
      onOk={handleOk}
      onCancel={handleCancel}
      okButtonProps={{ danger: true, loading }}
      okText="Delete"
    >
      <Typography.Paragraph type="danger">
        This action cannot be undone. Type <strong>{entityName}</strong> to confirm.
      </Typography.Paragraph>
      <Input
        value={inputValue}
        onChange={(e) => { setInputValue(e.target.value); setError(''); }}
        placeholder={`Type "${entityName}" to confirm`}
        status={error ? 'error' : undefined}
        onPressEnter={handleOk}
      />
      {error && (
        <Typography.Text type="danger" style={{ display: 'block', marginTop: 4 }}>
          {error}
        </Typography.Text>
      )}
    </Modal>
  );
}
