import { error, redirect } from "@sveltejs/kit"
import type { LayoutServerLoad } from "./$types"

export const load: LayoutServerLoad = async (event) => {
  const { session } = await event.parent();
  
  if (!session) {
    throw redirect(302, '/login');
  }

  const path = event.url.pathname;
  const isAdminOnly = ['/explore/users', '/explore/exclusions'].some(p => path.startsWith(p));
  const userRole = (session?.user as any)?.role || 'user';

  if (isAdminOnly && userRole !== 'admin') {
    throw error(403, 'Forbidden: Admin access required');
  }

  return {
    session
  }
}
