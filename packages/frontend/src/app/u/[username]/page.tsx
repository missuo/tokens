import type { Metadata } from 'next';
import { notFound, permanentRedirect } from 'next/navigation';
import ProfilePageClient from './ProfilePageClient';

export const revalidate = 60;

async function getProfileData(username: string) {
  // In production: use explicit URL or Vercel auto-URL.
  // In dev: use 127.0.0.1 to avoid ECONNREFUSED from localhost dual-stack DNS.
  const baseUrl = process.env.NEXT_PUBLIC_URL
    || (process.env.VERCEL_URL ? `https://${process.env.VERCEL_URL}` : null)
    || 'http://127.0.0.1:3000';
  
  const res = await fetch(`${baseUrl}/api/users/${username}`, {
    next: { revalidate: 60 },
  });
  
  if (!res.ok) {
    return null;
  }
  
  return res.json();
}

export async function generateMetadata({ params }: { params: Promise<{ username: string }> }): Promise<Metadata> {
  const { username } = await params;
  return {
    title: `@${username} - Token Usage | Tokens`,
    description: `View ${username}'s AI token usage statistics and cost breakdown on Tokens`,
    openGraph: {
      title: `@${username}'s Token Usage | Tokens`,
      description: `AI token usage statistics for ${username} on Tokens`,
      type: 'profile',
      url: `https://tokens.ci/u/${username}`,
      siteName: 'Tokens',
      images: [
        {
          url: 'https://tokens.ci/og-image.png',
          width: 1200,
          height: 630,
          alt: `${username}'s Token Usage on Tokens`,
        },
      ],
    },
    twitter: {
      card: 'summary_large_image',
      title: `@${username}'s Token Usage | Tokens`,
      images: ['https://tokens.ci/og-image.png'],
    },
  };
}

export default async function ProfilePage({ params }: { params: Promise<{ username: string }> }) {
  const { username } = await params;
  const data = await getProfileData(username);
  
  if (!data) {
    notFound();
  }

  if (data.user?.username && data.user.username !== username) {
    permanentRedirect(`/u/${data.user.username}`);
  }
  
  return <ProfilePageClient initialData={data} username={username} />;
}
