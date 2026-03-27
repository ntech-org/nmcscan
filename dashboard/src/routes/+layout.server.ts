import { error, redirect } from "@sveltejs/kit"
import type { LayoutServerLoad } from "./$types"

export const load: LayoutServerLoad = async (event) => {
  const session = await event.locals.auth();
  
  // Basic route protection
  const path = event.url.pathname;
  if (!session && path.startsWith('/admin')) {
    throw redirect(302, '/login');
  }

  // Admin-only sub-route protection (e.g., users, settings)
  const isAdminOnly = ['/admin/users', '/admin/exclusions'].some(p => path.startsWith(p));
  const userRole = (session?.user as any)?.role || 'user';

  if (isAdminOnly && userRole !== 'admin') {
    throw error(403, 'Forbidden: Admin access required');
  }

  return {
    session
  }
}
