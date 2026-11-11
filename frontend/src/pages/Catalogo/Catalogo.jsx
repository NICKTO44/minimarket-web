import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react';
import { Images, PackageX, Search, X } from 'lucide-react';
import { api, API_URL, claveDeSesion } from '../../api/api';
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
import { sinCategoriaCarta } from '../../utils/categoriaCarta';

// Módulo "Catálogo" (CATALOGO): los productos del inventario en tarjetas con
// foto, para que los vendedores revisen rápido. Solo se mira: no se abre,
// no se vende ni se edita nada desde aquí. El stock se vuelve a leer solo
// cada pocos segundos (y al volver a la pestaña): siempre el del inventario.

const POR_TANDA = 40;
const REFRESCO_MS = 30000;

// Lo último que se leyó, en memoria mientras siga la misma sesión en esta
// pestaña: al volver al Catálogo se muestra al instante y se actualiza por
// detrás (el stock, a los pocos segundos). Al cerrar sesión, cambiar de
// negocio o recargar la página se lee todo de nuevo.
let guardado = { sesion: null };
function deMemoria(campo, vacio) {
  return guardado.sesion && guardado.sesion === claveDeSesion() && guardado[campo] !== undefined ? guardado[campo] : vacio;
}
function aMemoria(campo, valor) {
  const sesion = claveDeSesion();
  if (guardado.sesion !== sesion) guardado = { sesion };
  guardado[campo] = valor;
}

/** Foto chica del catálogo ({id}_m.jpg, ver backend/handlers/imagenes.rs). */
function urlMiniatura(url) {
  return String(url).replace(/\.jpg(\?.*)?$/i, '_m.jpg$1');
}

function Imagen({ url, alt, onClick }) {
  // 0 = miniatura (400 px, liviana: el catálogo carga muchas a la vez),
  // 1 = foto grande (si aún no hay miniatura), 2 = sin foto.
  const [intento, setIntento] = useState(0);
  if (intento === 2) return <Images size={34} strokeWidth={1.4} className="cat-sin-imagen" aria-hidden="true" />;
  const src = `${API_URL}${intento === 0 ? urlMiniatura(url) : url}`;
  return <img src={src} alt={alt} loading="lazy" decoding="async" onClick={onClick} onError={() => setIntento((i) => i + 1)} />;
}

function Foto({ producto, foto2 }) {
  // Foto 1 (la de siempre) y, si tiene, la foto 2: se pasa con los puntos
  // de abajo (no tapan la foto) o tocando la foto. Al llegar a la última
  // vuelve a la primera.
  const fotos = [producto.imagen_url, foto2].filter(Boolean);
  const [cual, setCual] = useState(0);
  if (fotos.length === 0) {
    return (
      <div className="cat-foto cat-foto-vacia" aria-hidden="true">
        <Images size={34} strokeWidth={1.4} />
      </div>
    );
  }
  const actual = Math.min(cual, fotos.length - 1);
  const varias = fotos.length > 1;
  const mover = (paso) => setCual((actual + paso + fotos.length) % fotos.length);
  return (
    <div className={`cat-foto${varias ? ' cat-foto-varias' : ''}`}>
      <Imagen
        key={fotos[actual]}
        url={fotos[actual]}
        alt={varias ? `${producto.nombre} (foto ${actual + 1})` : producto.nombre}
        onClick={varias ? () => mover(1) : undefined}
      />
      {varias && (
        <div className="cat-puntos" role="group" aria-label="Fotos del producto">
          {fotos.map((f, i) => (
            <button
              key={f}
              type="button"
              className={`cat-punto${i === actual ? ' activo' : ''}`}
              aria-label={`Foto ${i + 1}`}
              aria-pressed={i === actual}
              onClick={() => setCual(i)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function Tarjeta({ p, ficha, compat, precios, foto2 }) {
  const sinStock = p.controla_stock !== false && Number(p.stock) <= 0;
  // Aplicación: la que viene en el producto (Excel de repuestos) o, si no,
  // los vehículos de su compatibilidad.
  const aplicacion = p.descripcion?.trim() || (compat || []).map(textoCompatibilidad).join(' / ');
  return (
    <article className={`cat-tarjeta${sinStock ? ' cat-agotado' : ''}`}>
      <Foto producto={p} foto2={foto2} />
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
  const [productos, setProductos] = useState(() => deMemoria('productos', null));
  const [categorias, setCategorias] = useState(() => deMemoria('categorias', []));
  const [repuestos, setRepuestos] = useState(() => deMemoria('repuestos', new Map()));
  const [compatibilidades, setCompatibilidades] = useState(() => deMemoria('compatibilidades', new Map()));
  const { mapa: preciosExtra } = usePreciosVenta(tresPrecios);
  // producto_id -> url de su foto 2 (el Catálogo solo se ve con su módulo).
  const [fotos2, setFotos2] = useState(() => deMemoria('fotos2', new Map()));
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
      const lista = (await api.productos()).filter((p) => p.activo !== false && !p.carta_dia);
      aMemoria('productos', lista);
      setProductos(lista);
      setError('');
    } catch (e) {
      setError(e.message || 'No se pudieron leer los productos.');
    }
  }, []);

  useEffect(() => {
    const primera = setTimeout(leerProductos, 0);
    api
      .categorias()
      .then((c) => {
        const lista = sinCategoriaCarta(c);
        aMemoria('categorias', lista);
        setCategorias(lista);
      })
      .catch(() => {});
    api
      .productoFotosExtra()
      .then((l) => {
        const mapa = new Map(l.filter((f) => f.orden === 2).map((f) => [f.producto_id, f.url]));
        aMemoria('fotos2', mapa);
        setFotos2(mapa);
      })
      .catch(() => {});
    if (fichaRepuesto) {
      api
        .repuestosFichas()
        .then((l) => {
          const mapa = mapaPorProducto(l);
          aMemoria('repuestos', mapa);
          setRepuestos(mapa);
        })
        .catch(() => {});
    }
    if (compatibilidad) {
      api
        .compatibilidades()
        .then((l) => {
          const mapa = mapaCompatibilidades(l);
          aMemoria('compatibilidades', mapa);
          setCompatibilidades(mapa);
        })
        .catch(() => {});
    }
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
              foto2={fotos2.get(p.id)}
            />
          ))}
        </div>
      )}
      {visibles.length < filtrados.length && <div ref={fin} className="cat-fin" />}
    </div>
  );
}
