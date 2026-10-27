import type { Metadata } from 'next';
import DeviceClient from './DeviceClient';

export const metadata: Metadata = {
  title: 'Device Authorization - Token Usage',
  description: 'Authorize your device to sync token usage data',
};

/**
 * Never let a shared cache hold this page.
 *
 * Prerendered, this route is absent from `middleware.ts`'s matcher — and a path
 * missing there never reaches the function, so nothing overrode Next's default
 * `s-maxage=31536000` for a fully static page. Cloudflare cached the document
 * for a year while every deploy reissued the JS chunks under new content
 * hashes, so the cached HTML went on asking for chunks the running image no
 * longer contained: a 404 served as `text/plain`, a refused script, and the
 * client-side exception that replaced the whole page. It reproduced per edge
 * location, which is why it looked like it depended on the visitor's IP.
 *
 * `force-dynamic` makes Next answer `private, no-store`, which is the right
 * answer for a step in an authentication flow. The cost is rendering a client
 * component's shell per request, on a page reached only while signing in.
 */
export const dynamic = "force-dynamic";

export default function DevicePage() {
  return <DeviceClient />;
}
