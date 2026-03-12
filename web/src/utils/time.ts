import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import localizedFormat from 'dayjs/plugin/localizedFormat';

dayjs.extend(utc);
dayjs.extend(localizedFormat);

export function formatTime(utcStr: string): string {
  return dayjs.utc(utcStr).local().format('YYYY-MM-DD HH:mm:ss');
}

export function formatTimeShort(utcStr: string): string {
  return dayjs.utc(utcStr).local().format('YYYY-MM-DD HH:mm');
}

export function formatRelative(utcStr: string): string {
  const now = dayjs();
  const target = dayjs.utc(utcStr).local();
  const diffMin = now.diff(target, 'minute');
  if (diffMin < 1) return '刚刚';
  if (diffMin < 60) return `${diffMin}分钟前`;
  const diffHour = now.diff(target, 'hour');
  if (diffHour < 24) return `${diffHour}小时前`;
  const diffDay = now.diff(target, 'day');
  if (diffDay < 30) return `${diffDay}天前`;
  return formatTime(utcStr);
}
