import { useState } from 'react';
import { API_URL } from '../api/api';
import { Store, LogOut } from 'lucide-react';
import { etiquetaRol, gruposParaUsuario, nivelAvisoSuscripcion } from '../utils/menu';
import './Sidebar.css';

// Sidebar de computadora/tablet grande. En celular (<= 899px) se oculta y
// la navegación la hace NavegacionMovil (barra inferior + panel "Más").

export default function Sidebar({
  pantalla,
  onCambiarPantalla,
  usuario,
  onLogout,
  nombreTienda = 'Mi Minimarket',
  ruc,
  diasRestantesSuscripcion,
  logoUrl,
  versionLogo,
  restaurante = false,
  etiquetas = {},
  modulos = [],
  // Números sobre un módulo (p. ej. { MESAS: 2 } = 2 pedidos listos).
  insignias = {},
}) {
  // Si la imagen del logo no carga (archivo borrado, sin conexión...),
  // se vuelve al ícono de tienda en lugar de mostrar una imagen rota.
  const [logoFallido, setLogoFallido] = useState(null);
  const mostrarLogo = !!logoUrl && logoFallido !== `${logoUrl}?v=${versionLogo}`;

  const seleccionar = (id) => onCambiarPantalla(id);

  const rolLabel = etiquetaRol(usuario);

  // Solo se muestra un aviso cuando de verdad importa: pocos días o ya
  // vencido. Si tiene vencimiento indefinido (null) o le queda tiempo de
  // sobra, no se distrae al usuario con nada.
  const nivelAviso = nivelAvisoSuscripcion(diasRestantesSuscripcion);
  const claseAvisoSuscripcion = nivelAviso ? `sidebar-punto-${nivelAviso}` : null;

  return (
    <>
      <aside className="sidebar">
        <div className="sidebar-marca">
          {mostrarLogo ? (
            <img
              className="sidebar-marca-logo"
              src={`${API_URL}${logoUrl}?v=${versionLogo}`}
              alt={nombreTienda}
              onError={() => setLogoFallido(`${logoUrl}?v=${versionLogo}`)}
            />
          ) : (
            <div className="sidebar-marca-icono">
              <Store size={20} />
            </div>
          )}
          <div className="sidebar-marca-texto">
            <span className="sidebar-marca-nombre">{nombreTienda}</span>
            {ruc && <span className="sidebar-marca-ruc">RUC {ruc}</span>}
          </div>
        </div>

        <nav className="sidebar-nav">
          {gruposParaUsuario(usuario, { restaurante, etiquetas, modulos }).map((grupo) => (
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
                    {insignias[item.id] > 0 && <span className="sidebar-insignia">{insignias[item.id]}</span>}
                    {item.id === 'SUSCRIPCION' && claseAvisoSuscripcion && (
                      <span className={`sidebar-punto-aviso ${claseAvisoSuscripcion}`} />
                    )}
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