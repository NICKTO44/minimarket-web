import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { API_URL } from '../api/api';
import { Store, LogOut, ChevronDown } from 'lucide-react';
import { etiquetaRol, gruposParaUsuario, nivelAvisoSuscripcion } from '../utils/menu';
import './MenuArriba.css';

// Módulo "Menú arriba": el mismo menú del Sidebar, en una barra horizontal.
// Solo se ve en computadoras y laptops (pantalla ancha con mouse o
// touchpad); en tablets y celulares sigue el Sidebar / NavegacionMovil
// (las reglas están en MenuArriba.css). Una sola fila: lo que no entra
// pasa a "Más ▾", que se calcula con el ancho real de cada opción.

const ESPACIO = 2; // gap entre opciones (px), igual que en el CSS

// En la barra van primero las pantallas de todos los días (en una laptop
// solo entran unas 9; Catálogo va junto a Productos y solo aparece con
// su módulo encendido); el resto sigue en el orden del menú y, si no cabe,
// queda en "Más".
const PRIMERO = ['MESAS', 'POS', 'PRODUCTOS', 'CATALOGO', 'CLIENTES', 'CAJA', 'COMPROBANTES', 'PROVEEDORES', 'STOCK', 'RESUMEN'];
const orden = (id) => (PRIMERO.includes(id) ? PRIMERO.indexOf(id) : PRIMERO.length);

export default function MenuArriba({
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
  const [logoFallido, setLogoFallido] = useState(null);
  const mostrarLogo = !!logoUrl && logoFallido !== `${logoUrl}?v=${versionLogo}`;

  const items = gruposParaUsuario(usuario, { restaurante, etiquetas, modulos })
    .flatMap((g) => g.items)
    .map((item, i) => ({ item, i }))
    .sort((a, b) => orden(a.item.id) - orden(b.item.id) || a.i - b.i)
    .map(({ item }) => item);
  const nivelAviso = nivelAvisoSuscripcion(diasRestantesSuscripcion);

  // Cuántas opciones entran en la fila (el resto va a "Más").
  const navRef = useRef(null);
  const medidasRef = useRef(null);
  const masRef = useRef(null);
  const [visibles, setVisibles] = useState(items.length);
  const [abierto, setAbierto] = useState(null); // 'MAS' | 'USUARIO' | null
  const menuRef = useRef(null);

  const claveItems = items.map((i) => `${i.id}:${i.label}`).join('|');

  useLayoutEffect(() => {
    const nav = navRef.current;
    const medidas = medidasRef.current;
    if (!nav || !medidas) return undefined;
    const calcular = () => {
      const anchos = [...medidas.children].map((el) => el.getBoundingClientRect().width);
      const disponible = nav.clientWidth;
      const total = anchos.reduce((s, a) => s + a + ESPACIO, 0);
      if (total <= disponible) {
        setVisibles(anchos.length);
        return;
      }
      const ancho_mas = (masRef.current?.getBoundingClientRect().width || 80) + ESPACIO;
      let usado = ancho_mas;
      let n = 0;
      for (const a of anchos) {
        if (usado + a + ESPACIO > disponible) break;
        usado += a + ESPACIO;
        n += 1;
      }
      setVisibles(n);
    };
    calcular();
    const observador = new ResizeObserver(calcular);
    observador.observe(nav);
    return () => observador.disconnect();
  }, [claveItems]);

  // Cerrar los desplegables al tocar fuera o con Escape.
  useEffect(() => {
    if (!abierto) return undefined;
    const fuera = (e) => {
      if (menuRef.current && !menuRef.current.contains(e.target)) setAbierto(null);
    };
    const tecla = (e) => e.key === 'Escape' && setAbierto(null);
    document.addEventListener('mousedown', fuera);
    document.addEventListener('keydown', tecla);
    return () => {
      document.removeEventListener('mousedown', fuera);
      document.removeEventListener('keydown', tecla);
    };
  }, [abierto]);

  const seleccionar = (item) => {
    if (item.proximamente) return;
    setAbierto(null);
    onCambiarPantalla(item.id);
  };

  const enFila = items.slice(0, visibles);
  const enMas = items.slice(visibles);
  const activoEnMas = enMas.some((i) => i.id === pantalla);

  const contenidoItem = (item) => {
    const Icono = item.icono;
    return (
      <>
        <Icono size={15} strokeWidth={2} />
        <span>{item.label}</span>
        {insignias[item.id] > 0 && <span className="ma-insignia">{insignias[item.id]}</span>}
        {item.id === 'SUSCRIPCION' && nivelAviso && <span className={`ma-punto ma-punto-${nivelAviso}`} />}
      </>
    );
  };

  return (
    <header className="menu-arriba" ref={menuRef}>
      <div className="ma-marca" title={nombreTienda}>
        {mostrarLogo ? (
          <img
            className="ma-logo"
            src={`${API_URL}${logoUrl}?v=${versionLogo}`}
            alt={nombreTienda}
            onError={() => setLogoFallido(`${logoUrl}?v=${versionLogo}`)}
          />
        ) : (
          <span className="ma-logo ma-logo-icono">
            <Store size={16} />
          </span>
        )}
        <span className="ma-nombre">{nombreTienda}</span>
      </div>

      <nav className="ma-nav" ref={navRef}>
        {enFila.map((item) => (
          <button
            key={item.id}
            className={`ma-item${pantalla === item.id ? ' activo' : ''}${item.proximamente ? ' proximamente' : ''}`}
            onClick={() => seleccionar(item)}
            disabled={item.proximamente}
          >
            {contenidoItem(item)}
          </button>
        ))}
        {enMas.length > 0 && (
          <div className="ma-mas">
            <button
              ref={masRef}
              className={`ma-item${activoEnMas ? ' activo' : ''}`}
              aria-expanded={abierto === 'MAS'}
              onClick={() => setAbierto((a) => (a === 'MAS' ? null : 'MAS'))}
            >
              <span>Más</span>
              <ChevronDown size={14} />
            </button>
            {abierto === 'MAS' && (
              <div className="ma-desplegable ma-desplegable-mas">
                {enMas.map((item) => (
                  <button
                    key={item.id}
                    className={`ma-opcion${pantalla === item.id ? ' activo' : ''}`}
                    onClick={() => seleccionar(item)}
                    disabled={item.proximamente}
                  >
                    {contenidoItem(item)}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
        {/* Copia invisible de todas las opciones: solo para medir su ancho. */}
        <div className="ma-medidas" ref={medidasRef} aria-hidden="true">
          {items.map((item) => (
            <span key={item.id} className="ma-item">
              {contenidoItem(item)}
            </span>
          ))}
        </div>
      </nav>

      <div className="ma-usuario-caja">
        <button
          className="ma-avatar"
          aria-expanded={abierto === 'USUARIO'}
          title={usuario.nombre}
          onClick={() => setAbierto((a) => (a === 'USUARIO' ? null : 'USUARIO'))}
        >
          {usuario.nombre.charAt(0).toUpperCase()}
        </button>
        {abierto === 'USUARIO' && (
          <div className="ma-desplegable ma-desplegable-usuario">
            <div className="ma-usuario-datos">
              <span className="ma-usuario-nombre">{usuario.nombre}</span>
              <span className="ma-usuario-rol">{etiquetaRol(usuario)}</span>
            </div>
            <button className="ma-opcion" onClick={onLogout}>
              <LogOut size={15} strokeWidth={2} />
              <span>Cerrar sesión</span>
            </button>
          </div>
        )}
      </div>
    </header>
  );
}
