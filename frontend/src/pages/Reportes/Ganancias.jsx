import { useEffect, useMemo, useState } from 'react';
import { api } from '../../api/api';
import { hoyLima } from '../../utils/formato';
import './Ganancias.css';

// Reporte de ganancias (módulo GANANCIAS, solo administrador).
// Ganancia = lo vendido menos lo que costó esa mercadería, todo con IGV
// incluido. El costo de cada venta queda guardado al cobrar; el servidor
// corta los meses en hora de Perú.

const MESES = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre'];

/** "2026-10" -> "octubre 2026" */
const nombreMes = (clave) => `${MESES[Number(clave.slice(5, 7)) - 1]} ${clave.slice(0, 4)}`;
/** "2026-10" -> "oct" */
const mesCorto = (clave) => MESES[Number(clave.slice(5, 7)) - 1].slice(0, 3);
const mayuscula = (texto) => texto.charAt(0).toUpperCase() + texto.slice(1);

const numero = (valor) => valor.toLocaleString('es-PE', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const soles = (valor) => (valor < 0 ? `− S/ ${numero(-valor)}` : `S/ ${numero(valor)}`);
const porcentaje = (valor) => (valor == null ? '—' : `${valor.toLocaleString('es-PE', { maximumFractionDigits: 1 })} %`);
const cantidad = (valor) => valor.toLocaleString('es-PE', { maximumFractionDigits: 3 });
/** Para el eje y la etiqueta del gráfico: 1250 -> "1.3 mil". */
const compacto = (valor) =>
  valor >= 1000 ? `${(valor / 1000).toLocaleString('es-PE', { maximumFractionDigits: 1 })} mil` : valor.toLocaleString('es-PE', { maximumFractionDigits: 0 });

/** Mes "2026-10" más o menos N meses. */
function moverMes(clave, meses) {
  const total = Number(clave.slice(0, 4)) * 12 + Number(clave.slice(5, 7)) - 1 + meses;
  return `${Math.floor(total / 12)}-${String((total % 12) + 1).padStart(2, '0')}`;
}

/** Tope redondo para el eje (1, 1.5, 2, 3... por una potencia de 10). */
function topeRedondo(maximo) {
  if (!(maximo > 0)) return 100;
  const base = 10 ** Math.floor(Math.log10(maximo));
  return [1, 1.5, 2, 3, 4, 5, 6, 8, 10].map((n) => n * base).find((t) => t >= maximo);
}

const margenDe = (ganancia, vendido) => (vendido > 0.005 ? Math.round((ganancia / vendido) * 1000) / 10 : null);

const COLUMNAS_PRODUCTO = [
  ['vendido', 'Vendido'],
  ['costo', 'Costo'],
  ['ganancia', 'Ganancia'],
  ['margen', 'Margen'],
];

export default function Ganancias({ onVerSinPrecio }) {
  const mesActual = hoyLima().slice(0, 7);
  const [mes, setMes] = useState(mesActual);
  const [datos, setDatos] = useState(null);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [vista, setVista] = useState('PRODUCTOS');
  const [orden, setOrden] = useState('ganancia');
  // Mes sobre el que está el puntero (o el foco) en el gráfico.
  const [mesSenalado, setMesSenalado] = useState(null);

  useEffect(() => {
    let vigente = true;
    api
      .reportesGanancias(mes)
      .then((respuesta) => {
        if (!vigente) return;
        setDatos(respuesta);
        setError(null);
      })
      .catch((e) => vigente && setError(e.message))
      .finally(() => vigente && setCargando(false));
    return () => {
      vigente = false;
    };
  }, [mes]);

  const elegirMes = (nuevo) => {
    if (nuevo === mes || nuevo > mesActual) return;
    setCargando(true);
    setMesSenalado(null);
    setMes(nuevo);
  };

  // Las tallas y colores de un modelo se suman en una sola fila.
  const productos = useMemo(() => {
    const grupos = new Map();
    for (const p of datos?.productos || []) {
      const clave = p.modelo ? `m:${p.modelo}` : `p:${p.producto_id}`;
      const g = grupos.get(clave) || {
        clave,
        nombre: p.modelo || p.nombre,
        categoria: p.categoria,
        cantidad: 0,
        vendido: 0,
        costo: 0,
        sinCosto: false,
        estimado: false,
      };
      g.cantidad += p.cantidad;
      g.vendido += p.vendido;
      if (p.costo == null) g.sinCosto = true;
      else g.costo += p.costo;
      g.estimado = g.estimado || p.estimado;
      grupos.set(clave, g);
    }
    const lista = [...grupos.values()].map((g) => ({
      ...g,
      ganancia: g.sinCosto ? null : g.vendido - g.costo,
      margen: g.sinCosto ? null : margenDe(g.vendido - g.costo, g.vendido),
    }));
    // Los que no tienen precio de compra van al final.
    const valor = (g) => (g[orden] == null ? -Infinity : g[orden]);
    return lista.sort((a, b) => valor(b) - valor(a) || b.vendido - a.vendido);
  }, [datos, orden]);

  const categorias = useMemo(() => {
    const grupos = new Map();
    for (const p of datos?.productos || []) {
      const g = grupos.get(p.categoria) || { nombre: p.categoria, vendido: 0, costo: 0, sinCosto: 0 };
      if (p.costo == null) g.sinCosto += p.vendido;
      else {
        g.vendido += p.vendido;
        g.costo += p.costo;
      }
      grupos.set(p.categoria, g);
    }
    return [...grupos.values()]
      .map((g) => ({ ...g, ganancia: g.vendido - g.costo, margen: margenDe(g.vendido - g.costo, g.vendido) }))
      .sort((a, b) => b.ganancia - a.ganancia);
  }, [datos]);

  if (!datos) {
    return error ? <p className="rep-mensaje">{error}</p> : <p className="rep-cargando">Cargando...</p>;
  }

  const meses = datos.meses;
  const actual = meses[meses.length - 1];
  const anterior = meses[meses.length - 2];
  const hayAnterior = anterior.vendido_con_costo > 0.005;
  const diferencia = actual.ganancia - anterior.ganancia;
  const sinVentas = !(Math.abs(actual.vendido) > 0.005) && datos.productos.length === 0;

  // Gráfico: una columna por mes; su alto es lo vendido (con costo conocido),
  // partido en costo (abajo) y ganancia (arriba). Un solo eje, en soles.
  const tope = topeRedondo(Math.max(...meses.map((m) => Math.max(m.vendido_con_costo, m.costo))));
  const alto = (valor) => `${Math.max(0, Math.min(100, (valor / tope) * 100))}%`;

  return (
    <div className={`gan${cargando ? ' gan-actualizando' : ''}`}>
      <div className="gan-mes">
        <button type="button" onClick={() => elegirMes(moverMes(mes, -1))} aria-label="Mes anterior">
          ‹
        </button>
        <strong>{mayuscula(nombreMes(datos.mes))}</strong>
        <button type="button" onClick={() => elegirMes(moverMes(mes, 1))} disabled={mes >= mesActual} aria-label="Mes siguiente">
          ›
        </button>
        {mes !== mesActual && (
          <button type="button" className="gan-mes-hoy" onClick={() => elegirMes(mesActual)}>
            Ir a este mes
          </button>
        )}
      </div>

      {error && <p className="rep-mensaje">{error}</p>}

      {datos.productos_sin_precio > 0 && (
        <div className="gan-aviso">
          <p>
            <strong>
              {datos.productos_sin_precio === 1
                ? '1 producto no tiene precio de compra.'
                : `${datos.productos_sin_precio} productos no tienen precio de compra.`}
            </strong>{' '}
            Si quieres un reporte de ganancias completo, no te olvides de ponerlo: lo que se venda de esos productos no
            entra en la ganancia.
          </p>
          {onVerSinPrecio && (
            <button type="button" onClick={onVerSinPrecio}>
              Completar en Productos
            </button>
          )}
        </div>
      )}

      <div className="rep-tarjetas gan-tarjetas">
        <div className="rep-tarjeta">
          <span className="rep-tarjeta-label">Vendido</span>
          <strong className="rep-tarjeta-valor">{soles(actual.vendido)}</strong>
          {actual.sin_costo > 0.005 && <span className="rep-tarjeta-sub">{soles(actual.sin_costo)} sin precio de compra</span>}
        </div>
        <div className="rep-tarjeta">
          <span className="rep-tarjeta-label">Costo de lo vendido</span>
          <strong className="rep-tarjeta-valor">{soles(actual.costo)}</strong>
          <span className="rep-tarjeta-sub">Lo que te costó esa mercadería</span>
        </div>
        <div className="rep-tarjeta rep-tarjeta-destacada">
          <span className="rep-tarjeta-label">Ganancia</span>
          <strong className="rep-tarjeta-valor">{soles(actual.ganancia)}</strong>
          {hayAnterior ? (
            <span className={`gan-cambio ${diferencia >= 0 ? 'sube' : 'baja'}`}>
              {diferencia >= 0 ? '▲' : '▼'} {soles(Math.abs(diferencia))} {diferencia >= 0 ? 'más' : 'menos'} que en{' '}
              {MESES[Number(anterior.mes.slice(5, 7)) - 1]}
            </span>
          ) : (
            <span className="rep-tarjeta-sub">Sin ventas con costo el mes anterior</span>
          )}
        </div>
        <div className="rep-tarjeta">
          <span className="rep-tarjeta-label">Margen</span>
          <strong className="rep-tarjeta-valor">{porcentaje(actual.margen)}</strong>
          <span className="rep-tarjeta-sub">De cada S/ 100 vendidos</span>
        </div>
        <div className="rep-tarjeta">
          <span className="rep-tarjeta-label">Compras del mes</span>
          <strong className="rep-tarjeta-valor">{soles(actual.compras)}</strong>
          <span className="rep-tarjeta-sub">Mercadería que entró</span>
        </div>
      </div>

      {(actual.sin_costo > 0.005 || actual.estimado || datos.devoluciones.monto > 0.005) && (
        <ul className="gan-notas">
          {actual.sin_costo > 0.005 && (
            <li>
              {soles(actual.sin_costo)} de lo vendido es de productos sin precio de compra: no entra en el costo ni en la
              ganancia.
            </li>
          )}
          {actual.estimado && (
            <li>
              Parte de estas ventas es anterior a la activación del reporte: su costo es <strong>estimado</strong> con el
              precio de compra de hoy.
            </li>
          )}
          {datos.devoluciones.monto > 0.005 && (
            <li>
              Ya están restadas las devoluciones del mes: {soles(datos.devoluciones.monto)} de venta y{' '}
              {soles(datos.devoluciones.costo_recuperado)} de costo (lo que volvió al stock).
            </li>
          )}
        </ul>
      )}

      <div className="rep-seccion">
        <div className="gan-cabecera">
          <h2>Comparativa de 12 meses</h2>
          <ul className="gan-leyenda" aria-label="Qué significa cada color">
            <li>
              <span className="gan-muestra gan-muestra-ganancia" /> Ganancia
            </li>
            <li>
              <span className="gan-muestra gan-muestra-costo" /> Costo de lo vendido
            </li>
          </ul>
        </div>
        <div className="gan-tarjeta">
          <div className="gan-grafico">
            <div className="gan-eje" aria-hidden="true">
              {[1, 0.5, 0].map((parte) => (
                <span key={parte} style={{ bottom: `${parte * 100}%` }}>
                  {parte === 0 ? '0' : compacto(tope * parte)}
                </span>
              ))}
            </div>
            <div className="gan-columnas">
              <div className="gan-lineas" aria-hidden="true">
                {[1, 0.5].map((parte) => (
                  <i key={parte} style={{ bottom: `${parte * 100}%` }} />
                ))}
              </div>
              {meses.map((m, indice) => {
                const elegido = m.mes === datos.mes;
                const ganancia = Math.max(m.ganancia, 0);
                const costo = m.ganancia < 0 ? m.costo : Math.min(m.costo, m.vendido_con_costo);
                return (
                  <button
                    key={m.mes}
                    type="button"
                    className={`gan-columna${elegido ? ' elegida' : ''}`}
                    onClick={() => elegirMes(m.mes)}
                    // El globo es para el puntero y el teclado; con el dedo, tocar
                    // la columna elige ese mes y sus números quedan a la vista.
                    onPointerEnter={(e) => e.pointerType !== 'touch' && setMesSenalado(m.mes)}
                    onPointerLeave={() => setMesSenalado(null)}
                    onFocus={(e) => e.target.matches(':focus-visible') && setMesSenalado(m.mes)}
                    onBlur={() => setMesSenalado(null)}
                    aria-label={`${nombreMes(m.mes)}: vendido ${soles(m.vendido_con_costo)}, costo ${soles(m.costo)}, ganancia ${soles(m.ganancia)}`}
                  >
                    <span className="gan-barra">
                      {elegido && (ganancia > 0 || costo > 0) && <b className="gan-etiqueta">{compacto(m.ganancia)}</b>}
                      {ganancia > 0 && <i className="gan-segmento gan-segmento-ganancia" style={{ height: alto(ganancia) }} />}
                      {costo > 0 && <i className="gan-segmento gan-segmento-costo" style={{ height: alto(costo) }} />}
                    </span>
                    <span className="gan-columna-mes">{mesCorto(m.mes)}</span>
                    {mesSenalado === m.mes && (
                      <span className={`gan-globo${indice < 3 ? ' izquierda' : indice > 8 ? ' derecha' : ''}`} role="tooltip">
                        <b>{mayuscula(nombreMes(m.mes))}</b>
                        <span>
                          <i className="gan-clave gan-muestra-ganancia" /> <strong>{soles(m.ganancia)}</strong> ganancia
                        </span>
                        <span>
                          <i className="gan-clave gan-muestra-costo" /> <strong>{soles(m.costo)}</strong> costo
                        </span>
                        <span>
                          <strong>{soles(m.vendido_con_costo)}</strong> vendido · margen {porcentaje(m.margen)}
                        </span>
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="rep-tabla-wrapper gan-tabla-meses">
            <table className="rep-tabla">
              <thead>
                <tr>
                  <th>Mes</th>
                  <th className="gan-num">Vendido</th>
                  <th className="gan-num">Costo</th>
                  <th className="gan-num">Ganancia</th>
                  <th className="gan-num">Margen</th>
                  <th className="gan-num">Compras</th>
                </tr>
              </thead>
              <tbody>
                {[...meses].reverse().map((m) => (
                  <tr key={m.mes} className={m.mes === datos.mes ? 'gan-fila-elegida' : ''}>
                    <td>
                      <button type="button" className="gan-enlace-mes" onClick={() => elegirMes(m.mes)}>
                        {mayuscula(nombreMes(m.mes))}
                      </button>
                      {m.estimado && <span className="gan-marca">estimado</span>}
                    </td>
                    <td className="gan-num">{soles(m.vendido)}</td>
                    <td className="gan-num">{soles(m.costo)}</td>
                    <td className={`gan-num gan-fuerte${m.ganancia < 0 ? ' gan-perdida' : ''}`}>{soles(m.ganancia)}</td>
                    <td className="gan-num">{porcentaje(m.margen)}</td>
                    <td className="gan-num">{soles(m.compras)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div className="rep-seccion">
        <div className="gan-cabecera">
          <h2>
            {vista === 'PRODUCTOS' ? 'Productos' : 'Categorías'} de {nombreMes(datos.mes)}
          </h2>
          <div className="gan-conmutador" role="tablist" aria-label="Ver por">
            {[
              ['PRODUCTOS', 'Por producto'],
              ['CATEGORIAS', 'Por categoría'],
            ].map(([valor, texto]) => (
              <button
                key={valor}
                type="button"
                role="tab"
                aria-selected={vista === valor}
                className={vista === valor ? 'activo' : ''}
                onClick={() => setVista(valor)}
              >
                {texto}
              </button>
            ))}
          </div>
        </div>

        <div className="rep-tabla-wrapper">
          {vista === 'PRODUCTOS' ? (
            <table className="rep-tabla">
              <thead>
                <tr>
                  <th>Producto</th>
                  <th className="gan-num">Cant.</th>
                  {COLUMNAS_PRODUCTO.map(([campo, texto]) => (
                    <th key={campo} className="gan-num" aria-sort={orden === campo ? 'descending' : undefined}>
                      <button type="button" className={`gan-orden${orden === campo ? ' activo' : ''}`} onClick={() => setOrden(campo)}>
                        {texto} {orden === campo ? '▾' : ''}
                      </button>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {productos.map((p) => (
                  <tr key={p.clave}>
                    <td>
                      {p.nombre}
                      {p.estimado && <span className="gan-marca">estimado</span>}
                      <small className="gan-categoria">{p.categoria}</small>
                    </td>
                    <td className="gan-num">{cantidad(p.cantidad)}</td>
                    <td className="gan-num">{soles(p.vendido)}</td>
                    {p.sinCosto ? (
                      <td className="gan-num gan-sin-costo" colSpan={3}>
                        Sin precio de compra
                      </td>
                    ) : (
                      <>
                        <td className="gan-num">{soles(p.costo)}</td>
                        <td className={`gan-num gan-fuerte${p.ganancia < 0 ? ' gan-perdida' : ''}`}>{soles(p.ganancia)}</td>
                        <td className="gan-num">{porcentaje(p.margen)}</td>
                      </>
                    )}
                  </tr>
                ))}
                {productos.length === 0 && (
                  <tr>
                    <td colSpan={6} className="rep-sin-resultados">
                      {sinVentas ? 'No hay ventas en este mes.' : 'Sin productos vendidos en este mes.'}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          ) : (
            <table className="rep-tabla">
              <thead>
                <tr>
                  <th>Categoría</th>
                  <th className="gan-num">Vendido</th>
                  <th className="gan-num">Costo</th>
                  <th className="gan-num">Ganancia</th>
                  <th className="gan-num">Margen</th>
                </tr>
              </thead>
              <tbody>
                {categorias.map((c) => (
                  <tr key={c.nombre}>
                    <td>
                      {c.nombre}
                      {c.sinCosto > 0.005 && <small className="gan-categoria">+ {soles(c.sinCosto)} sin precio de compra</small>}
                    </td>
                    {c.vendido < 0.005 && c.sinCosto > 0.005 ? (
                      <td className="gan-num gan-sin-costo" colSpan={4}>
                        Sin precio de compra
                      </td>
                    ) : (
                      <>
                        <td className="gan-num">{soles(c.vendido)}</td>
                        <td className="gan-num">{soles(c.costo)}</td>
                        <td className={`gan-num gan-fuerte${c.ganancia < 0 ? ' gan-perdida' : ''}`}>{soles(c.ganancia)}</td>
                        <td className="gan-num">{porcentaje(c.margen)}</td>
                      </>
                    )}
                  </tr>
                ))}
                {categorias.length === 0 && (
                  <tr>
                    <td colSpan={5} className="rep-sin-resultados">
                      No hay ventas en este mes.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          )}
        </div>
      </div>

      <p className="gan-pie">
        Ganancia = lo vendido menos lo que costó esa mercadería, con IGV incluido en ambos. El costo es el promedio de lo
        que había en la tienda el día de cada venta. No incluye otros gastos del negocio (alquiler, sueldos, servicios).
      </p>
    </div>
  );
}
