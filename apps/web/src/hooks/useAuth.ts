import { useCallback, useEffect, useState } from "react";
import { authApi, clearAuth, getStoredUser, setAuth, subscribeToAuthExpiry } from "@/api/client";
import type { AuthUser } from "@/types";

export function useAuth() {
  const [user, setUser] = useState<AuthUser | null>(getStoredUser);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [initializing, setInitializing] = useState(true);

  useEffect(() => {
    let disposed = false;

    const unsubscribe = subscribeToAuthExpiry(() => {
      if (disposed) return;
      setUser(null);
      setError("登录状态已过期，请重新登录。");
    });

    void authApi
      .me()
      .then((nextUser) => {
        if (disposed) return;
        setAuth("", nextUser);
        setUser(nextUser);
        setError(null);
      })
      .catch(() => {
        if (disposed) return;
        clearAuth();
        setUser(null);
      })
      .finally(() => {
        if (!disposed) setInitializing(false);
      });

    return () => {
      disposed = true;
      unsubscribe();
    };
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await authApi.login(email, password);
      setAuth(res.token, res.user);
      setUser(res.user);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  const register = useCallback(async (email: string, password: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await authApi.register(email, password);
      setAuth(res.token, res.user);
      setUser(res.user);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  const logout = useCallback(() => {
    void authApi.logout().catch(() => undefined).finally(() => {
      clearAuth();
      setUser(null);
    });
  }, []);

  return { user, error, loading, initializing, login, register, logout };
}
