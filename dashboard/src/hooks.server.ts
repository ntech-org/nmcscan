import { SvelteKitAuth } from "@auth/sveltekit";
import Discord from "@auth/sveltekit/providers/discord";
import GitHub from "@auth/sveltekit/providers/github";
import PostgresAdapter from "@auth/pg-adapter";
import { env } from "$env/dynamic/private";
import { getPool, query } from "$lib/db";

export const { handle, signIn, signOut } = SvelteKitAuth({
  adapter: PostgresAdapter(getPool()),
  providers: [
    Discord({
      clientId: env.DISCORD_CLIENT_ID,
      clientSecret: env.DISCORD_CLIENT_SECRET,
      allowDangerousEmailAccountLinking: true,
    }),
    GitHub({
      clientId: env.GITHUB_CLIENT_ID,
      clientSecret: env.GITHUB_CLIENT_SECRET,
      allowDangerousEmailAccountLinking: true,
      profile(profile) {
        return {
          id: profile.id.toString(),
          name: profile.name || profile.login,
          email: profile.email,
          image: profile.avatar_url,
          role: "user",
        };
      },
    }),
  ],
  secret: env.AUTH_SECRET,
  trustHost: true,
  session: {
    strategy: "database",
  },
  callbacks: {
    async signIn({ user }) {
      const email = user.email?.toLowerCase();
      if (!email) {
        console.warn("🚫 SignIn attempt without email");
        return false;
      }

      const allowed =
        env.ALLOWED_USERS?.split(",").map((e) => e.trim().toLowerCase()) || [];
      if (allowed.includes(email)) {
        console.log(`✅ SignIn allowed for admin/allowed user: ${email}`);
        return true;
      }

      try {
        const res = await query("SELECT role FROM users WHERE email = $1", [
          email,
        ]);
        if (res.rows.length > 0) {
          const isAllowed = res.rows[0].role !== "blocked";
          console.log(
            `ℹ️ SignIn for existing user ${email}: role=${res.rows[0].role}, allowed=${isAllowed}`,
          );
          return isAllowed;
        }
      } catch (e) {
        console.error("❌ Database error during signIn check:", e);
      }

      console.warn(`🚫 SignIn rejected for unknown user: ${email}`);
      return false;
    },
    async session({ session, user }) {
      if (session.user) {
        (session.user as any).id = user.id;
        (session.user as any).role = (user as any).role || "user";
      }
      return session;
    },
  },
  events: {
    async createUser({ user }) {
      const email = user.email?.toLowerCase();
      const allowed =
        env.ALLOWED_USERS?.split(",").map((e) => e.trim().toLowerCase()) || [];

      if (email && allowed.includes(email)) {
        try {
          await query("UPDATE users SET role = $1 WHERE id = $2", [
            "admin",
            user.id,
          ]);
          console.log(`🚀 Bootstrapped Admin: ${email}`);
        } catch (e) {
          console.error("❌ Error bootstrapping admin:", e);
        }
      }
    },
  },
});
