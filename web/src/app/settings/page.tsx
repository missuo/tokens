import type { Metadata } from 'next';
import SettingsClient from './SettingsClient';
import { getTranslator } from '@/lib/i18n/server';

export async function generateMetadata(): Promise<Metadata> {
  const { t } = await getTranslator();

  return {
    title: t('settings.meta.title'),
    description: t('settings.meta.description'),
  };
}

export default function SettingsPage() {
  return <SettingsClient />;
}
