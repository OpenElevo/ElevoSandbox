import { Tabs } from 'antd';
import OidcSettings from './OidcSettings';

const items = [
  {
    key: 'oidc',
    label: 'SSO 单点登录',
    children: <OidcSettings />,
  },
];

export default function SystemSettings() {
  return (
    <div style={{ padding: 24 }}>
      <Tabs items={items} defaultActiveKey="oidc" />
    </div>
  );
}
