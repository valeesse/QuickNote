import { LoginPage } from "@/components/LoginPage";
import { MainApp } from "@/MainApp";
import { useAuth } from "@/hooks/useAuth";

export default function App() {
  const auth = useAuth();

  if (auth.initializing) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-gray-50 text-sm text-gray-500">
        正在恢复登录状态...
      </div>
    );
  }

  if (!auth.user) {
    return (
      <LoginPage
        onLogin={auth.login}
        onRegister={auth.register}
        error={auth.error}
        loading={auth.loading}
      />
    );
  }

  return <MainApp userEmail={auth.user.email} onLogout={auth.logout} />;
}
