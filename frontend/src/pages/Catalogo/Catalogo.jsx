import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react';
import { Images, PackageX, Search, X } from 'lucide-react';
import { api, API_URL } from '../../api/api';
import { textoPlano } from '../../utils/medicamentos';
import {
  coincidenTodas,
  compatibilidadCoincide,
  fichaRepuestoCoincide,
  mapaCompatibilidades,
  mapaPorProducto,
  palabrasDeBusqueda,
  textoCompatibilidad,
} from '../../utils/repuestos';
import { usePreciosVenta } from '../../utils/tresPrecios';
import { formatoCantidad, formatoPrecio } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import '../../components/PantallaModulo.css';
import './Catalogo.css';

// Módulo "Catálogo" (CATALOGO): los productos del inventario en tarjetas con
// foto, para que los vendedores revisen rápido. Solo se mira: no se abre,
// no se vende ni se edita nada desde aquí. El stock se vuelve a leer solo
// cada pocos segundos (y al volver a la pestaña): siempre el del inventario.

const POR_TANDA = 40;
const REFRESCO_MS = 30000;

/** Foto chica del catálogo ({id}_m.jpg, ver backend/handlers/imagenes.rs). */
function urlMiniatura(url) {
  return String(url).replace(/\.jpg(\?.*)?$/i, '_m.jpg$1');
}

function Foto({ producto }) {
  // 0 = miniatura, 1 = foto grande (si aún no hay miniatura), 2 = sin foto.
  const [intento, setIntento] = useState(0);
  if (!producto.imagen_url || intento === 2) {
    return (
      <div className="cat-foto cat-foto-vacia" aria-hidden="true">
        <Images size={34} strokeWidth={1.4} />
      </div>
    );
  }
  const src = `${API_URL}${intento === 0 ? urlMiniatura(producto.imagen_url) : producto.imagen_url}`;
  return (
    <div className="cat-foto">
      <img src={src} alt={producto.nombre} loading="lazy" decoding="async" onError={() => setIntento((i) => i + 1)} />
    </div>
  );
}

function Tarjeta({ p, ficha, compat, precios }) {
  const sinStock = p.controla_stock !== false && Number(p.stock) <= 0;
  // Aplicación: la que viene en el producto (Excel de repuestos) o, si no,
  // los vehículos de su compatibilidad.
  const aplicacion = p.descripcion?.trim() || (compat || []).map(textoCompatibilidad).join(' / ');
  return (
    <article className={`cat-tarjeta${sinStock ? ' cat-agotado' : ''}`}>
      <Foto producto={p} />
      <div className="cat-cuerpo">
        {ficha?.marca && <p className="cat-marca">{ficha.marca}</p>}
        <p className="cat-nombre">
          {p.nombre}
          {ficha?.numero_parte && <span className="cat-parte"> {ficha.numero_parte}</span>}
        </p>
        {aplicacion && <p className="cat-aplicacion">{aplicacion}</p>}
        {/* Los precios juntos y del mismo tamaño, cada uno con su color. */}
        <div className="cat-precios">
          {precios && (precios.precio_2 || precios.precio_3) ? (
            <>
              <span className="cat-precio cat-p1">
                <small>P1</small>S/ {formatoPrecio(p.precio)}
              </span>
              {precios.precio_2 ? (
                <span className="cat-precio cat-p2">
                  <small>P2</small>S/ {formatoPrecio(precios.precio_2)}
                </span>
              ) : null}
              {precios.precio_3 ? (
                <span className="cat-precio cat-p3">
                  <small>P3</small>S/ {formatoPrecio(precios.precio_3)}
                </span>
              ) : null}
            </>
          ) : (
            <span className="cat-precio cat-p1">S/ {formatoPrecio(p.precio)}</span>
          )}
        </div>
        {ficha?.ubicacion && <p className="cat-ubicacion">📍 {ficha.ubicacion}</p>}
      </div>
      <dl className="cat-pie">
        <div className={sinStock ? 'cat-rojo' : undefined}>
          <dt>Stock:</dt>
          <dd>{p.controla_stock === false ? '—' : formatoCantidad(p.stock)}</dd>
        </div>
        <div>
          <dt>Código:</dt>
          <dd>{p.codigo}</dd>
        </div>
        <div>
          <dt>U.M:</dt>
          <dd>{abreviaturaUnidad(p.unidad_medida).toUpperCase()}</dd>
        </div>
      </dl>
    </article>
  );
}

export default function Catalogo({ fichaRepuesto, compatibilidad, tresPrecios }) {
  const [productos, setProductos] = useState(null);
  const [categorias, setCategorias] = useState([]);
  const [repuestos, setRepuestos] = useState(() => new Map());
  const [compatibilidades, setCompatibilidades] = useState(() => new Map());
  const { mapa: preciosExtra } = usePreciosVenta(tresPrecios);
  const [error, setError] = useState('');

  const [busqueda, setBusqueda] = useState('');
  const [categoria, setCategoria] = useState('');
  const [marca, setMarca] = useState('');
  const [soloConStock, setSoloConStock] = useState(false);
  const consulta = useDeferredValue(busqueda);
  // Cuántas tarjetas se ven; cada filtro nuevo vuelve a la primera tanda.
  const claveFiltro = `${consulta}|${categoria}|${marca}|${soloConStock}`;
  const [tanda, setTanda] = useState({ clave: claveFiltro, n: POR_TANDA });
  const mostrar = tanda.clave === claveFiltro ? tanda.n : POR_TANDA;

  // Productos (con su stock): al entrar, cada 30 s mientras se ve la
  // pantalla y al volver a la pestaña.
  const leerProductos = useCallback(async () => {
    try {
      const lista = await api.productos();
      setProductos(lista.filter((p) => p.activo !== false && !p.carta_dia));
      setError('');
    } catch (e) {
      setError(e.message || 'No se pudieron leer los productos.');
    }
  }, []);

  useEffect(() => {
    const primera = setTimeout(leerProductos, 0);
    api
      .categorias()
      .then((c) => setCategorias(Array.isArray(c) ? c : []))
      .catch(() => {});
    if (fichaRepuesto) api.repuestosFichas().then((l) => setRepuestos(mapaPorProducto(l))).catch(() => {});
    if (compatibilidad) api.compatibilidades().then((l) => setCompatibilidades(mapaCompatibilidades(l))).catch(() => {});
    const t = setInterval(() => {
      if (document.visibilityState === 'visible') leerProductos();
    }, REFRESCO_MS);
    const alVolver = () => document.visibilityState === 'visible' && leerProductos();
    document.addEventListener('visibilitychange', alVolver);
    window.addEventListener('focus', alVolver);
    return () => {
      clearTimeout(primera);
      clearInterval(t);
      document.removeEventListener('visibilitychange', alVolver);
      window.removeEventListener('focus', alVolver);
    };
  }, [leerProductos, fichaRepuesto, compatibilidad]);

  // Texto de búsqueda de cada producto, armado una sola vez (no en cada letra).
  const indice = useMemo(() => {
    const m = new Map();
    for (const p of productos || []) {
      const f = repuestos.get(p.id);
      m.set(
        p.id,
        textoPlano(
          [p.nombre, p.codigo, p.descripcion, f?.marca, f?.numero_parte, f?.ubicacion, ...(compatibilidades.get(p.id) || []).map(textoCompatibilidad)]
            .filter(Boolean)
            .join(' ')
        )
      );
    }
    return m;
  }, [productos, repuestos, compatibilidades]);

  const marcas = useMemo(() => {
    const s = new Set();
    for (const f of repuestos.values()) if (f.marca?.trim()) s.add(f.marca.trim().toUpperCase());
    return [...s].sort((a, b) => a.localeCompare(b, 'es'));
  }, [repuestos]);

  // Mismo criterio que el punto de venta de repuestos: palabras en cualquier
  // orden y sin tildes; n.° de parte y códigos alternos con o sin guiones.
  const filtrados = useMemo(() => {
    let lista = productos || [];
    if (categoria) lista = lista.filter((p) => String(p.categoria_id) === categoria);
    if (marca) lista = lista.filter((p) => (repuestos.get(p.id)?.marca || '').trim().toUpperCase() === marca);
    if (soloConStock) lista = lista.filter((p) => p.controla_stock === false || Number(p.stock) > 0);
    const q = consulta.trim();
    if (!q) return lista;
    const plano = textoPlano(q);
    const palabras = palabrasDeBusqueda(q);
    return lista.filter(
      (p) =>
        coincidenTodas(palabras, indice.get(p.id)) ||
        (fichaRepuesto && fichaRepuestoCoincide(repuestos.get(p.id), plano)) ||
        (compatibilidad && compatibilidadCoincide(compatibilidades.get(p.id), plano))
    );
  }, [productos, categoria, marca, soloConStock, consulta, indice, repuestos, compatibilidades, fichaRepuesto, compatibilidad]);

  // Al acercarse al final se agregan más tarjetas.
  const fin = useRef(null);
  useEffect(() => {
    const el = fin.current;
    if (!el || !('IntersectionObserver' in window)) return undefined;
    const obs = new IntersectionObserver(
      (e) => e[0].isIntersecting && setTanda({ clave: claveFiltro, n: mostrar + POR_TANDA }),
      { rootMargin: '600px' }
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [filtrados.length, mostrar, claveFiltro]);

  const visibles = filtrados.slice(0, mostrar);

  return (
    <div className="pm-layout cat-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <Images size={22} /> Catálogo
          </h1>
          <p className="pm-subtitulo">
            {productos ? `${filtrados.length} de ${productos.length} productos` : 'Cargando...'} · el stock se actualiza solo
          </p>
        </div>
      </header>

      <div className="cat-filtros">
        <label className="cat-buscar">
          <Search size={18} />
          <input
            value={busqueda}
            onChange={(e) => setBusqueda(e.target.value)}
            placeholder={fichaRepuesto || compatibilidad ? 'Buscar por nombre, código, aplicación o N.° de parte...' : 'Buscar por nombre o código...'}
            autoFocus
          />
          {busqueda && (
            <button type="button" className="cat-limpiar" onClick={() => setBusqueda('')} aria-label="Borrar búsqueda">
              <X size={16} />
            </button>
          )}
        </label>
        <select value={categoria} onChange={(e) => setCategoria(e.target.value)} aria-label="Categoría">
          <option value="">Todas las categorías</option>
          {categorias.map((c) => (
            <option key={c.id} value={String(c.id)}>
              {c.nombre}
            </option>
          ))}
        </select>
        {marcas.length > 0 && (
          <select value={marca} onChange={(e) => setMarca(e.target.value)} aria-label="Marca">
            <option value="">Todas las marcas</option>
            {marcas.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        )}
        <label className="cat-check">
          <input type="checkbox" checked={soloConStock} onChange={(e) => setSoloConStock(e.target.checked)} /> Solo con stock
        </label>
      </div>

      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      {productos && filtrados.length === 0 ? (
        <div className="cat-vacio">
          <PackageX size={36} strokeWidth={1.4} />
          <p>No hay productos con esa búsqueda.</p>
        </div>
      ) : (
        <div className="cat-grilla">
          {visibles.map((p) => (
            <Tarjeta
              key={p.id}
              p={p}
              ficha={repuestos.get(p.id)}
              compat={compatibilidades.get(p.id)}
              precios={tresPrecios ? preciosExtra.get(p.id) : null}
            />
          ))}
        </div>
      )}
      {visibles.length < filtrados.length && <div ref={fin} className="cat-fin" />}
    </div>
  );
}
