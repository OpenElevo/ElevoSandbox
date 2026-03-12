import { Empty, Button, Typography } from 'antd';
import { ClearOutlined } from '@ant-design/icons';

interface EmptyStateNoDataProps {
  /** Which variant to render */
  variant: 'no-data';
  /** Description shown below the icon */
  description?: string;
  /** Label for the create button */
  createLabel?: string;
  /** Called when the create button is clicked */
  onCreate?: () => void;
}

interface EmptyStateNoResultsProps {
  /** Which variant to render */
  variant: 'no-results';
  /** Called when "清除筛选条件" link is clicked */
  onClearFilters?: () => void;
}

type EmptyStateProps = EmptyStateNoDataProps | EmptyStateNoResultsProps;

/**
 * EmptyState renders two variants:
 * - `no-data`: shown when a list is empty without any active filter.
 *   Displays an icon, description, and an optional create button.
 * - `no-results`: shown when a filtered list is empty.
 *   Displays "未找到匹配结果" and a "清除筛选条件" link.
 */
export default function EmptyState(props: EmptyStateProps) {
  if (props.variant === 'no-results') {
    return (
      <Empty
        image={Empty.PRESENTED_IMAGE_SIMPLE}
        description={
          <div style={{ textAlign: 'center' }}>
            <Typography.Text type="secondary">未找到匹配结果</Typography.Text>
            {props.onClearFilters && (
              <div style={{ marginTop: 8 }}>
                <Typography.Link
                  onClick={props.onClearFilters}
                  style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}
                >
                  <ClearOutlined />
                  清除筛选条件
                </Typography.Link>
              </div>
            )}
          </div>
        }
      />
    );
  }

  return (
    <Empty
      description={
        <Typography.Text type="secondary">
          {props.description ?? '暂无数据'}
        </Typography.Text>
      }
    >
      {props.onCreate && (
        <Button type="primary" onClick={props.onCreate}>
          {props.createLabel ?? '新建'}
        </Button>
      )}
    </Empty>
  );
}
