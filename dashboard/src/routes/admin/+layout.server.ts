import { error, redirect } from "@sveltejs/kit"
import type { LayoutServerLoad } from "./$types"

export const load: LayoutServerLoad = async (event) => {
  const { session } = await event.parent();

  if (!session) {
    throw redirect(302, '/login');
  }

  const userRole = (session?.user as any)?.role || 'user';
  if (userRole !== 'admin') {
    throw error(403, 'Forbidden: Admin access required');
  }

  return {
    session
  }
}
