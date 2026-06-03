import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { useCoreQuery, useCoreMutation } from "../../contexts/SpacedriveContext";
import { LanguageSelector } from "../../components/settings/LanguageSelector";

interface DeviceSettingsForm {
  name: string;
  slug: string;
}

export function GeneralSettings() {
  const { t } = useTranslation('settings');
  const { t: tc } = useTranslation('common');

  const statusQuery = useCoreQuery({ type: "core.status", input: null as any });
  const configQuery = useCoreQuery({ type: "config.app.get", input: null as any });
  const updateDevice = useCoreMutation("device.update");
  const resetData = useCoreMutation("core.reset");

  const { data: status } = statusQuery;
  const { data: config } = configQuery;

  const deviceForm = useForm<DeviceSettingsForm>({
    values: {
      name: status?.device_info?.name || "",
      slug: status?.device_info?.slug || "",
    },
  });

  const onDeviceSubmit = deviceForm.handleSubmit(async (data) => {
    await updateDevice.mutateAsync({
      name: data.name,
      slug: data.slug,
    });
    statusQuery.refetch();
  });

  const handleResetData = () => {
    const confirmed = window.confirm(
      t('resetData.confirmTitle') + '\n\n' + t('resetData.confirmMessage')
    );

    if (confirmed) {
      resetData.mutate(
        { confirm: true },
        {
          onSuccess: (result) => {
            alert(
              result.message || t('resetData.successMessage')
            );
          },
          onError: (error) => {
            alert(tc('status.error') + ': ' + (error.message || tc('status.error')));
          },
        }
      );
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-lg font-semibold text-ink mb-2">{t('general.title')}</h2>
        <p className="text-sm text-ink-dull">
          {t('general.description')}
        </p>
      </div>

      <div className="space-y-4">
        {/* Language */}
        <LanguageSelector />

        {/* Device Configuration */}
        <form onSubmit={onDeviceSubmit} className="p-4 bg-app-box rounded-lg border border-app-line space-y-4">
          <h3 className="text-sm font-medium text-ink">{t('device.title')}</h3>

          <label className="block">
            <span className="text-sm font-medium text-ink mb-1 block">{t('device.name')}</span>
            <p className="text-xs text-ink-dull mb-2">
              {t('device.nameDescription')}
            </p>
            <input
              type="text"
              {...deviceForm.register("name")}
              className="w-full px-3 py-2 bg-app border border-app-line rounded-md text-ink text-sm focus:outline-none focus:ring-2 focus:ring-accent"
              placeholder="My Computer"
            />
          </label>

          <label className="block">
            <span className="text-sm font-medium text-ink mb-1 block">{t('device.slug')}</span>
            <p className="text-xs text-ink-dull mb-2">
              {t('device.slugDescription')}
            </p>
            <input
              type="text"
              {...deviceForm.register("slug")}
              className="w-full px-3 py-2 bg-app border border-app-line rounded-md text-ink text-sm focus:outline-none focus:ring-2 focus:ring-accent font-mono"
              placeholder="my-computer"
            />
          </label>

          {deviceForm.formState.isDirty && (
            <button
              type="submit"
              disabled={updateDevice.isPending}
              className="px-4 py-2 bg-accent hover:bg-accent-deep text-white rounded-md text-sm font-medium transition-colors disabled:opacity-50"
            >
              {updateDevice.isPending ? tc('status.saving') : tc('actions.save')}
            </button>
          )}
        </form>

        {/* Version Info */}
        <div className="p-4 bg-app-box rounded-lg border border-app-line space-y-3">
          <h3 className="text-sm font-medium text-ink">{t('version.title')}</h3>
          <div className="flex justify-between items-center">
            <span className="text-sm text-ink">{t('version.version')}</span>
            <span className="text-sm text-ink-dull font-mono">
              {status?.version || tc('status.loading')}
            </span>
          </div>
          <div className="flex justify-between items-center">
            <span className="text-sm text-ink">{t('version.built')}</span>
            <span className="text-sm text-ink-dull font-mono">
              {status?.built_at || tc('status.loading')}
            </span>
          </div>
        </div>

        <div className="p-4 bg-app-box rounded-lg border border-app-line">
          <h3 className="text-sm font-medium text-ink mb-1">{t('dataDirectory.title')}</h3>
          <p className="text-xs text-ink-dull mb-2">{t('dataDirectory.description')}</p>
          <code className="block text-xs text-ink-dull bg-app rounded px-2 py-1 overflow-x-auto">
            {config?.data_dir || status?.system?.data_directory || tc('status.loading')}
          </code>
        </div>

        <div className="p-4 bg-app-box rounded-lg border border-app-line">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-ink mb-1">{t('resetData.title')}</h3>
              <p className="text-xs text-ink-dull">
                {t('resetData.description')}
              </p>
            </div>
            <button
              type="button"
              onClick={handleResetData}
              disabled={resetData.isPending}
              className="px-4 py-2 bg-red-600 hover:bg-red-700 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
            >
              {resetData.isPending ? tc('status.resetting') : tc('actions.reset')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
