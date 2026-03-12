import { App, Input, Modal, Typography } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { deleteTenant } from '@/api/tenants';
import type { Tenant } from '@/types';

interface UseDeleteTenantOptions {
  // When provided, navigate to this path after successful deletion
  navigateAfterDelete?: string;
}

export function useDeleteTenant(options: UseDeleteTenantOptions = {}) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { message, modal } = App.useApp();
  const { navigateAfterDelete } = options;

  const onDeleted = () => {
    queryClient.invalidateQueries({ queryKey: ['tenants'] });
    message.success('租户已删除');
    if (navigateAfterDelete) {
      navigate(navigateAfterDelete);
    }
  };

  const handleDelete = async (tenant: Tenant) => {
    // First attempt: try deleting without force to probe state
    try {
      await deleteTenant(tenant.id, false);
      onDeleted();
      return;
    } catch (err: unknown) {
      const error = err as { response?: { status?: number; data?: { error?: { code?: string; message?: string } } } };
      const status = error.response?.status;
      const code = error.response?.data?.error?.code;
      const msg = error.response?.data?.error?.message;

      if (status !== 409) {
        message.error(msg || '删除租户失败');
        return;
      }

      // Case 1: has active shares or sandboxes → hard block
      if (code === 'HAS_ACTIVE_SHARES' || code === 'HAS_ACTIVE_SANDBOXES') {
        Modal.error({
          title: '无法删除租户',
          content: msg || '该租户仍有活跃的 Share 或 Sandbox，请先清理后再删除。',
        });
        return;
      }

      // Case 2: has active API keys → warning, require name confirm, force=true
      if (code === 'HAS_ACTIVE_API_KEYS') {
        let inputName = '';
        modal.confirm({
          title: `删除租户「${tenant.name}」？`,
          content: (
            <div>
              <Typography.Text type="warning" style={{ display: 'block', marginBottom: 8 }}>
                {msg || '该租户有活跃 API Key，删除后这些 Key 将永久失效。'}
              </Typography.Text>
              <Typography.Text type="danger" style={{ display: 'block', marginBottom: 8 }}>
                此操作不可逆，请谨慎操作。
              </Typography.Text>
              <Input
                placeholder="请输入租户名称确认"
                onChange={(e) => { inputName = e.target.value; }}
              />
            </div>
          ),
          okText: '确认删除',
          okButtonProps: { danger: true },
          cancelText: '取消',
          onOk: async () => {
            if (inputName !== tenant.name) {
              message.error('名称不匹配');
              throw new Error('mismatch');
            }
            await deleteTenant(tenant.id, true);
            onDeleted();
          },
        });
        return;
      }

      // Case 3: other 409 / clean tenant — normal confirmation
      let inputName = '';
      modal.confirm({
        title: `删除租户「${tenant.name}」？`,
        content: (
          <div>
            <Typography.Text type="danger">此操作不可逆，请谨慎操作。</Typography.Text>
            <Input
              placeholder="请输入租户名称确认"
              style={{ marginTop: 8 }}
              onChange={(e) => { inputName = e.target.value; }}
            />
          </div>
        ),
        okText: '删除',
        okButtonProps: { danger: true },
        cancelText: '取消',
        onOk: async () => {
          if (inputName !== tenant.name) {
            message.error('名称不匹配');
            throw new Error('mismatch');
          }
          await deleteTenant(tenant.id, true);
          onDeleted();
        },
      });
    }
  };

  return handleDelete;
}
