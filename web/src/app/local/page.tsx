import type { Metadata } from 'next';
import LocalClient from './LocalClient';
import { getTranslator } from '@/lib/i18n/server';

export async function generateMetadata(): Promise<Metadata> {
  const { t } = await getTranslator();

  return {
    title: t('settings.local.meta.title'),
    description: t('settings.local.meta.description'),
  };
}

export default function LocalViewerPage() {
  return <LocalClient />;
}
