import { useEffect, useState, useCallback } from 'react';
import { Input, Button, Space } from 'antd';
import { SearchOutlined, ReloadOutlined } from '@ant-design/icons';

interface ListToolbarProps {
  /** Current search value (controlled from parent) */
  searchValue?: string;
  /** Debounce delay in ms, default 300 */
  debounce?: number;
  /** Placeholder for search input */
  searchPlaceholder?: string;
  /** Called with debounced search value */
  onSearch?: (value: string) => void;
  /** Called when reset button is clicked — clears search and any filters */
  onReset?: () => void;
  /** Additional filter slots rendered between search and reset */
  filters?: React.ReactNode;
  /** Extra controls rendered at the end (e.g. create button) */
  extra?: React.ReactNode;
}

export default function ListToolbar({
  searchValue = '',
  debounce = 300,
  searchPlaceholder = '搜索...',
  onSearch,
  onReset,
  filters,
  extra,
}: ListToolbarProps) {
  const [inputValue, setInputValue] = useState(searchValue);

  // Sync input if parent resets the value externally
  useEffect(() => {
    setInputValue(searchValue);
  }, [searchValue]);

  // Debounce callback
  useEffect(() => {
    if (!onSearch) return;
    const timer = setTimeout(() => {
      onSearch(inputValue);
    }, debounce);
    return () => clearTimeout(timer);
  }, [inputValue, debounce, onSearch]);

  const handleReset = useCallback(() => {
    setInputValue('');
    onReset?.();
  }, [onReset]);

  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16, gap: 8, flexWrap: 'wrap' }}>
      <Space wrap>
        {onSearch !== undefined && (
          <Input
            prefix={<SearchOutlined />}
            placeholder={searchPlaceholder}
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            allowClear
            style={{ width: 300 }}
          />
        )}
        {filters}
        {onReset && (
          <Button icon={<ReloadOutlined />} onClick={handleReset}>
            重置
          </Button>
        )}
      </Space>
      {extra && <Space>{extra}</Space>}
    </div>
  );
}
