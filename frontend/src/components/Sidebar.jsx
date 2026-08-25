import { useState } from 'react';
import {
  ScanBarcode,
  LayoutGrid,
  Wallet,
  History,
  Package,
  Boxes,
  Truck,
  Undo2,
  Receipt,
  BarChart3,
  Settings,
  Store,
  LogOut,
  Menu,
  Users,
} from 'lucide-react';
import './Sidebar.css';

const GRUPOS = [
  {
    titulo: 'Operación',
    items: [
      { id: 'POS', label: 'Punto de Venta', icono: ScanBarcode },
      { id: 'RESUMEN', label: 'Resumen', icono: LayoutGrid },
      { id: 'CAJA', label: 'Caja y Turnos', icono: Wallet },
      { id: 'HISTORIAL_CAJA', label: 'Historial de Caja', icono: History },
      { id: 'CLIENTES', label: 'Clientes', icono: Users },
    ],
  },
  {
    titulo: 'Inventario',
    items: [
      { id: 'PRODUCTOS', label: 'Productos', icono: Package },
      { id: 'STOCK', label: 'Stock y Lotes', icono: Boxes },
      { id: 'PROVEEDORES', label: 'Proveedores', icono: Truck },
      { id: 'DEVOLUCIONES', label: 'Devoluciones', icono: Undo2 },
    ],
  },
  {
    titulo: 'Administración',
    items: [
      { id: 'COMPROBANTES', label: 'Comprobantes', icono: Receipt },
      { id: 'REPORTES', label: 'Reportes', icono: BarChart3 },
      { id: 'CONFIGURACION', label: 'Configuración', icono: Settings },
    ],
  },
];

export default function Sidebar({ pantalla, onCambiarPantalla, usuario, onLogout, nombreTienda = 'Mi Minimarket', ruc }) {
  const [abierto, setAbierto] = useState(false);

  const seleccionar = (id) => {
    onCambiarPantalla(id);
    setAbierto(false);
  };

  const rolLabel = usuario.rol_id === 1 ? 'Administrador' : 'Cajero';

  return (
    <>
      <button className="sidebar-toggle-movil" onClick={() => setAbierto(true)}>
        <Menu size={20} />
      </button>

      {abierto && <div className="sidebar-overlay" onClick={() => setAbierto(false)} />}

      <aside className={`sidebar ${abierto ? 'sidebar-abierto' : ''}`}>
        <div className="sidebar-marca">
          <div className="sidebar-marca-icono">
            <Store size={20} />
          </div>
          <div className="sidebar-marca-texto">
            <span className="sidebar-marca-nombre">{nombreTienda}</span>
            {ruc && <span className="sidebar-marca-ruc">RUC {ruc}</span>}
          </div>
        </div>

        <nav className="sidebar-nav">
          {GRUPOS.map((grupo) => (
            <div className="sidebar-grupo" key={grupo.titulo}>
              <span className="sidebar-grupo-titulo">{grupo.titulo}</span>
              {grupo.items.map((item) => {
                const Icono = item.icono;
                return (
                  <button
                    key={item.id}
                    className={`sidebar-item ${pantalla === item.id ? 'activo' : ''} ${item.proximamente ? 'proximamente' : ''}`}
                    onClick={() => !item.proximamente && seleccionar(item.id)}
                    disabled={item.proximamente}
                  >
                    <Icono size={18} strokeWidth={2} />
                    <span className="sidebar-item-label">{item.label}</span>
                  </button>
                );
              })}
            </div>
          ))}
        </nav>

        <div className="sidebar-pie">
          <div className="sidebar-usuario">
            <span className="sidebar-usuario-avatar">{usuario.nombre.charAt(0).toUpperCase()}</span>
            <div className="sidebar-usuario-texto">
              <span className="sidebar-usuario-nombre">{usuario.nombre}</span>
              <span className="sidebar-usuario-rol">{rolLabel}</span>
            </div>
          </div>
          <button className="sidebar-logout" onClick={onLogout}>
            <LogOut size={18} strokeWidth={2} />
            <span>Cerrar sesión</span>
          </button>
        </div>
      </aside>
    </>
  );
}