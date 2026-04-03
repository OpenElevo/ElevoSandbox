import { useState, useEffect, useCallback } from 'react';
import { Form, Input, InputNumber, Switch, Button, App, Space, Divider, Alert, Typography } from 'antd';
import { getOidcFullConfig, updateOidcConfig, testOidcConfig, type OidcConfigDisplay, type OidcConfigUpdateParams } from '@/api/oidcConfig';

export default function OidcSettings() {
  const [form] = Form.useForm();
  const { message, modal } = App.useApp();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [config, setConfig] = useState<OidcConfigDisplay | null>(null);

  const fetchConfig = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getOidcFullConfig();
      setConfig(data);
      form.setFieldsValue({
        enabled: data.enabled,
        issuer_url: data.issuer_url,
        client_id: data.client_id,
        client_secret: data.client_secret,
        redirect_uri: data.redirect_uri,
        jwks_refresh_interval_secs: data.jwks_refresh_interval_secs,
        disable_password_login: data.disable_password_login,
        auto_create_tenant: data.auto_create_tenant,
      });
    } catch {
      message.error('加载 OIDC 配置失败');
    } finally {
      setLoading(false);
    }
  }, [form, message]);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  const handleSave = async (values: OidcConfigDisplay & { client_secret?: string }) => {
    setSaving(true);
    try {
      const params: OidcConfigUpdateParams = {
        enabled: values.enabled,
        issuer_url: values.issuer_url,
        client_id: values.client_id,
        redirect_uri: values.redirect_uri,
        jwks_refresh_interval_secs: values.jwks_refresh_interval_secs,
        disable_password_login: form.getFieldValue('disable_password_login') ?? false,
        auto_create_tenant: values.auto_create_tenant,
      };
      // Only include client_secret if the user changed it (not the masked value)
      if (values.client_secret && values.client_secret !== '••••••••' && values.client_secret.trim()) {
        params.client_secret = values.client_secret;
      }
      await updateOidcConfig(params);
      message.success('OIDC 配置已保存');
      await fetchConfig();
    } catch {
      message.error('保存 OIDC 配置失败');
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    try {
      const result = await testOidcConfig();
      if (result.success) {
        message.success(result.message || '连接测试成功');
      } else {
        message.error(result.message || '连接测试失败');
      }
    } catch {
      message.error('连接测试失败');
    } finally {
      setTesting(false);
    }
  };

  const enabled = Form.useWatch('enabled', form);
  const disablePasswordLogin = Form.useWatch('disable_password_login', form);

  const handleDisablePasswordChange = (checked: boolean) => {
    if (checked) {
      modal.confirm({
        title: '确认禁用密码登录',
        content: '禁用密码登录后，所有用户只能通过 SSO 登录。如果 SSO 配置异常，可能导致无法登录管理后台。确定要继续吗？',
        okText: '确认禁用',
        okType: 'danger',
        cancelText: '取消',
        onOk: () => {
          form.setFieldsValue({ disable_password_login: true });
        },
      });
    } else {
      form.setFieldsValue({ disable_password_login: false });
    }
  };

  if (loading) {
    return <div style={{ padding: 24, textAlign: 'center', color: '#999' }}>加载中...</div>;
  }

  return (
    <div style={{ maxWidth: 640 }}>
      <Typography.Title level={5} style={{ marginTop: 0, marginBottom: 16 }}>OIDC 单点登录配置</Typography.Title>

      {config && config.enabled && (
        <Alert
          type="info"
          showIcon
          message="SSO 已启用"
          description={`回调地址: ${config.redirect_uri}`}
          style={{ marginBottom: 16 }}
        />
      )}

      <Form
        form={form}
        layout="vertical"
        onFinish={handleSave}
        onValuesChange={(changed) => {
          // When disabling OIDC, also disable disable_password_login
          if ('enabled' in changed && !changed.enabled) {
            form.setFieldsValue({ disable_password_login: false });
          }
        }}
      >
        <Form.Item name="enabled" label="启用 SSO" valuePropName="checked">
          <Switch />
        </Form.Item>

        <Divider orientation="left" plain style={{ fontSize: 13 }}>连接配置</Divider>

        <Form.Item
          name="issuer_url"
          label="Issuer URL"
          rules={[
            { required: true, message: '请输入 Issuer URL' },
            { type: 'url', message: '请输入有效的 URL 地址' },
          ]}
          tooltip="OIDC Provider 的 issuer 地址，如 https://elevo.example.com/oidc"
        >
          <Input placeholder="https://elevo.example.com/oidc" />
        </Form.Item>

        <Form.Item
          name="client_id"
          label="Client ID"
          rules={[{ required: true, message: '请输入 Client ID' }]}
        >
          <Input placeholder="your-client-id" />
        </Form.Item>

        <Form.Item
          name="client_secret"
          label="Client Secret"
          tooltip="留空或保持当前值不变则不会更新"
        >
          <Input.Password placeholder={config?.client_secret || '请输入 Client Secret'} />
        </Form.Item>

        <Form.Item
          name="redirect_uri"
          label="回调地址 (Redirect URI)"
          tooltip="此地址需在 OIDC Provider 中注册。一般自动推导即可，如有反向代理可手动修改"
        >
          <Input placeholder="https://example.com/api/v1/auth/oidc/callback" />
        </Form.Item>

        <Divider orientation="left" plain style={{ fontSize: 13 }}>高级配置</Divider>

        <Form.Item
          name="jwks_refresh_interval_secs"
          label="JWKS 刷新间隔（秒）"
          tooltip="定期从 OIDC Provider 拉取公钥"
        >
          <InputNumber min={60} max={3600} step={30} style={{ width: '100%' }} placeholder="300" />
        </Form.Item>

        <Form.Item
          label="禁用密码登录"
          tooltip="启用后，用户只能通过 SSO 登录，密码登录将被禁用"
          extra={!enabled ? '需要先启用 SSO' : undefined}
        >
          <Switch
            checked={!!disablePasswordLogin}
            disabled={!enabled}
            onChange={handleDisablePasswordChange}
          />
        </Form.Item>

        <Form.Item
          name="auto_create_tenant"
          label="自动创建租户"
          tooltip="当 OIDC 用户首次通过 API 访问时，自动创建对应租户"
          valuePropName="checked"
        >
          <Switch />
        </Form.Item>

        <Divider />

        <Form.Item style={{ marginBottom: 0 }}>
          <Space>
            <Button type="primary" htmlType="submit" loading={saving}>
              保存配置
            </Button>
            <Button onClick={handleTest} loading={testing} disabled={!enabled}>
              测试连接
            </Button>
          </Space>
        </Form.Item>
      </Form>
    </div>
  );
}
