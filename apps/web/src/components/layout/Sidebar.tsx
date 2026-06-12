import { BarChart3, ClipboardList, HelpCircle, History, LayoutDashboard, LogOut, MessagesSquare } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export type AppView = "resumen" | "hilos" | "revision" | "anteriores" | "reportes" | "ayuda";

type Props = {
  view: AppView;
  onNavigate: (view: AppView) => void;
  email: string;
  reviewCount: number;
  onLogout: () => void;
};

const navItems: Array<{ view: AppView; label: string; icon: LucideIcon }> = [
  { view: "resumen", label: "Resumen", icon: LayoutDashboard },
  { view: "hilos", label: "Hilos", icon: MessagesSquare },
  { view: "revision", label: "Revisión manual", icon: ClipboardList },
  { view: "anteriores", label: "Análisis anteriores", icon: History },
  { view: "reportes", label: "Reportes", icon: BarChart3 },
  { view: "ayuda", label: "Ayuda", icon: HelpCircle },
];

export function Sidebar({ view, onNavigate, email, reviewCount, onLogout }: Props) {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <img src="/logo-192.png" alt="Gmail Inspector" />
        <strong>Gmail Inspector</strong>
        <span>Análisis de correos</span>
      </div>
      <nav className="sidebar-nav" aria-label="Navegación principal">
        {navItems.map((item) => (
          <button
            type="button"
            key={item.view}
            className={item.view === view ? "nav-item active" : "nav-item"}
            onClick={() => onNavigate(item.view)}
          >
            <item.icon size={18} />
            <span>{item.label}</span>
            {item.view === "revision" && reviewCount > 0 && <span className="nav-badge">{reviewCount}</span>}
          </button>
        ))}
      </nav>
      <div className="sidebar-user">
        <img src="/logo-192.png" alt="" />
        <div className="sidebar-user-info">
          <span className="sidebar-user-email">{email}</span>
        </div>
        <button type="button" className="logout-button" onClick={onLogout}>
          <LogOut size={15} />
          <span>Cerrar sesión</span>
        </button>
      </div>
    </aside>
  );
}
