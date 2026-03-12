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
      setError('名称不匹配');
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
      okText="删除"
      cancelText="取消"
    >
      <Typography.Paragraph type="danger">
        此操作不可逆。请输入 <strong>{entityName}</strong> 确认删除。
      </Typography.Paragraph>
      <Input
        value={inputValue}
        onChange={(e) => { setInputValue(e.target.value); setError(''); }}
        placeholder={`请输入"${entityName}"确认`}
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
