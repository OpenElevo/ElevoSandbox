import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import localizedFormat from 'dayjs/plugin/localizedFormat';

dayjs.extend(utc);
dayjs.extend(localizedFormat);

export function formatTime(utcStr: string): string {
  return dayjs.utc(utcStr).local().format('YYYY-MM-DD HH:mm:ss');
}

export function formatRelative(utcStr: string): string {
  const now = dayjs();
  const target = dayjs.utc(utcStr).local();
  const diffMin = now.diff(target, 'minute');
  if (diffMin < 1) return 'just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHour = now.diff(target, 'hour');
  if (diffHour < 24) return `${diffHour}h ago`;
  const diffDay = now.diff(target, 'day');
  if (diffDay < 30) return `${diffDay}d ago`;
  return formatTime(utcStr);
}
