import { useEffect, useState } from 'react';
import { ChefHat, ClipboardList, House, LayoutGrid, LogOut, Package, ScanBarcode, Store, UtensilsCrossed, Wallet } from 'lucide-react';
import { API_URL } from '../api/api';
import { esMesero, esPreparacion, etiquetaRol, gruposParaUsuario, nivelAvisoSuscripcion } from '../utils/menu';
import './NavegacionMovil.css';

// ============================================================
// Navegación en celular (<= 899px), como una app:
//  - Barra superior con el logo y nombre del negocio.
//  - Barra inferior: Inicio · Caja · [VENDER] · Productos · Más.
//  - "Más" abre un panel desde abajo con el resto de módulos.
// En computadora esto no se muestra (se usa el Sidebar).
// ============================================================

// Las 4 secciones fijas de la barra (el resto va en "Más"). En una
// cafetería/restaurante, "Mesas" reemplaza a "Inicio"; el mesero solo
// tiene Mesas.
const TABS_TIENDA = ['RESUMEN', 'CAJA', 'POS', 'PRODUCTOS'];
const TABS_RESTAURANTE = ['MESAS', 'CAJA', 'POS', 'PRODUCTOS'];
const TABS_MESERO = ['MESAS', 'CARTA'];
const TABS_PREPARACION = ['PREPARACION', 'CARTA'];

// Detecta si el teclado del celular está abierto (el alto visible baja
// mucho) para ocultar la barra inferior y que no quede encima del teclado.
function useTecladoAbierto() {
  const [abierto, setAbierto] = useState(false);
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return undefined;
    let altoMaximo = vv.height;
    const revisar = () => {
      altoMaximo = Math.max(altoMaximo, vv.height);
      setAbierto(altoMaximo - vv.height > 150);
    };
    vv.addEventListener('resize', revisar);
    return () => vv.removeEventListener('resize', revisar);
  }, []);
  return abierto;
}

function Tab({ id, label, Icono, pantalla, onIr, insignia = 0 }) {
  return (
    <button
      type="button"
      className={`navm-tab${pantalla === id ? ' activo' : ''}`}
      onClick={() => onIr(id)}
      aria-current={pantalla === id ? 'page' : undefined}
    >
      <span className="navm-tab-icono">
        <Icono size={20} strokeWidth={2} />
        {insignia > 0 && <span className="navm-insignia">{insignia}</span>}
      </span>
      {label}
    </button>
  );
}

export default function NavegacionMovil({
  pantalla,
  onCambiarPantalla,
  usuario,
  onLogout,
  nombreTienda = 'Mi Minimarket',
  diasRestantesSuscripcion,
  logoUrl,
  versionLogo,
  restaurante = false,
  etiquetas = {},
  modulos = [],
  insignias = {},
}) {
  const [masAbierto, setMasAbierto] = useState(false);
  const [logoFallido, setLogoFallido] = useState(null);
  const tecladoAbierto = useTecladoAbierto();

  const esAdmin = usuario.rol_id === 1;
  const rolLabel = etiquetaRol(usuario);
  const mesero = restaurante && esMesero(usuario);
  const preparacion = restaurante && esPreparacion(usuario);
  const tabsFijas = preparacion
    ? TABS_PREPARACION
    : mesero
      ? TABS_MESERO
      : restaurante
        ? TABS_RESTAURANTE
        : TABS_TIENDA;
  const srcLogo = logoUrl ? `${API_URL}${logoUrl}?v=${versionLogo}` : null;
  const mostrarLogo = !!srcLogo && logoFallido !== srcLogo;

  // Aviso de suscripción: solo lo ve el administrador (es quien puede pagar).
  const nivelAviso = esAdmin ? nivelAvisoSuscripcion(diasRestantesSuscripcion) : null;

  // "Más" = todos los módulos que ese usuario puede ver, menos los fijos.
  const gruposMas = gruposParaUsuario(usuario, { restaurante, etiquetas, modulos })
    .map((g) => ({ ...g, items: g.items.filter((i) => !tabsFijas.includes(i.id)) }))
    .filter((g) => g.items.length > 0);
  const masActivo = !tabsFijas.includes(pantalla);

  useEffect(() => {
    if (!masAbierto) return undefined;
    const alPresionarTecla = (e) => e.key === 'Escape' && setMasAbierto(false);
    window.addEventListener('keydown', alPresionarTecla);
    return () => window.removeEventListener('keydown', alPresionarTecla);
  }, [masAbierto]);

  const ir = (id) => {
    onCambiarPantalla(id);
    setMasAbierto(false);
  };

  return (
    <>
      <header className="navm-superior">
        {mostrarLogo ? (
          <img className="navm-logo" src={srcLogo} alt={nombreTienda} onError={() => setLogoFallido(srcLogo)} />
        ) : (
          <span className="navm-logo navm-logo-icono">
            <Store size={18} />
          </span>
        )}
        <div className="navm-negocio">
          <strong>{nombreTienda}</strong>
          <span>
            {usuario.nombre} · {rolLabel}
          </span>
        </div>
      </header>

      <nav className={`navm-inferior${tecladoAbierto ? ' navm-oculta' : ''}`} aria-label="Navegación principal">
        {preparacion ? (
          <Tab id="PREPARACION" label="Preparación" Icono={ChefHat} pantalla={pantalla} onIr={ir} />
        ) : restaurante ? (
          <Tab id="MESAS" label="Mesas" Icono={UtensilsCrossed} pantalla={pantalla} onIr={ir} insignia={insignias.MESAS} />
        ) : (
          <Tab id="RESUMEN" label="Inicio" Icono={House} pantalla={pantalla} onIr={ir} />
        )}
        {(mesero || preparacion) && (
          <Tab id="CARTA" label="Carta de hoy" Icono={ClipboardList} pantalla={pantalla} onIr={ir} />
        )}
        {!mesero && !preparacion && (
          <>
            <Tab id="CAJA" label="Caja" Icono={Wallet} pantalla={pantalla} onIr={ir} />
            <button
              type="button"
              className={`navm-vender${pantalla === 'POS' ? ' activo' : ''}`}
              onClick={() => ir('POS')}
              aria-current={pantalla === 'POS' ? 'page' : undefined}
            >
              <span className="navm-vender-circulo">
                <ScanBarcode size={26} strokeWidth={2} />
              </span>
              Vender
            </button>
            <Tab id="PRODUCTOS" label={etiquetas.PRODUCTOS || 'Productos'} Icono={Package} pantalla={pantalla} onIr={ir} />
          </>
        )}
        <button
          type="button"
          className={`navm-tab${masActivo || masAbierto ? ' activo' : ''}`}
          onClick={() => setMasAbierto(true)}
          aria-haspopup="dialog"
          aria-expanded={masAbierto}
        >
          <span className="navm-tab-icono">
            <LayoutGrid size={20} strokeWidth={2} />
            {nivelAviso && <span className={`navm-punto navm-punto-${nivelAviso}`} />}
          </span>
          Más
        </button>
      </nav>

      {masAbierto && (
        <div className="navm-velo" onMouseDown={(e) => e.target === e.currentTarget && setMasAbierto(false)}>
          <div className="navm-panel" role="dialog" aria-modal="true" aria-label="Más opciones">
            <div className="navm-asa" aria-hidden="true" />
            {gruposMas.map((grupo) => (
              <div key={grupo.titulo}>
                <span className="navm-grupo-titulo">{grupo.titulo}</span>
                <div className="navm-grilla">
                  {grupo.items.map((item) => {
                    const Icono = item.icono;
                    return (
                      <button
                        key={item.id}
                        type="button"
                        className={`navm-modulo${pantalla === item.id ? ' activo' : ''}`}
                        onClick={() => ir(item.id)}
                      >
                        <span className="navm-modulo-icono">
                          <Icono size={20} strokeWidth={2} />
                          {item.id === 'SUSCRIPCION' && nivelAviso && (
                            <span className={`navm-punto navm-punto-${nivelAviso}`} />
                          )}
                        </span>
                        {item.label}
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}

            <div className="navm-usuario">
              <span className="navm-avatar">{usuario.nombre.charAt(0).toUpperCase()}</span>
              <div className="navm-usuario-texto">
                <strong>{usuario.nombre}</strong>
                <span>{rolLabel}</span>
              </div>
              <button type="button" className="navm-salir" onClick={onLogout}>
                <LogOut size={16} strokeWidth={2} />
                Salir
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
