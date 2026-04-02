import { error } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { env } from '$env/dynamic/private';

const BACKEND_URL = env.BACKEND_URL || 'http://localhost:3000';
const API_KEY = env.API_KEY || '';

export const fallback: RequestHandler = async (event) => {
  const { params, request, url } = event;
  const path = params.path;

  const clientApiKey = request.headers.get('X-API-Key');
  const session = await event.locals.auth();

  // Allow requests with a valid API key (external tools) or a browser session
  if (!clientApiKey && !session && path !== 'info') {
    throw error(401, 'Unauthorized: No session or API key');
  }

  // Forward everything to the Rust backend
  const targetUrl = `${BACKEND_URL}/api/${path}${url.search}`;

  const headers = new Headers(request.headers);

  if (clientApiKey) {
    // External tool: forward their API key as-is, let the backend validate it
    headers.set('X-API-Key', clientApiKey);
  } else if (API_KEY) {
    // Dashboard session: use master key + user ID for impersonation
    headers.set('X-API-Key', API_KEY);
    if (session && session.user) {
      headers.set('X-User-Id', (session.user as any).id.toString());
    }
  }

  // Remove host header to avoid issues with target
  headers.delete('host');
  headers.delete('connection');
  headers.delete('cookie'); // AuthJS session cookie is not needed by Rust backend

  try {
    const res = await fetch(targetUrl, {
      method: request.method,
      headers: headers,
      body: request.method !== 'GET' && request.method !== 'HEAD' ? await request.arrayBuffer() : undefined,
      duplex: 'half'
    } as any);

    return new Response(res.body, {
      status: res.status,
      headers: {
        'Content-Type': res.headers.get('Content-Type') || 'application/json',
        'Cache-Control': res.headers.get('Cache-Control') || 'no-cache'
      }
    });
  } catch (e) {
    console.error(`Proxy error for ${targetUrl}:`, e);
    throw error(502, 'Bad Gateway: Backend unreachable');
  }
};
