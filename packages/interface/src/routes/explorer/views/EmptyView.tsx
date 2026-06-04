import { useTranslation } from 'react-i18next';

export function EmptyView() {
  const { t } = useTranslation('explorer');

  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center">
        <div className="text-ink-dull text-sm">
          {t('empty.selectLocation')}
        </div>
      </div>
    </div>
  );
}