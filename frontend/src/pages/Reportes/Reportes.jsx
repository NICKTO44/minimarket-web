import { useState, useEffect, useRef } from 'react';
import { api } from '../../api/api';
import './Reportes.css';
import { nombreMetodo, partesDelPago } from '../../utils/metodoPago';
import { fechaHoraLima, hoyLima } from '../../utils/formato';
import Ganancias from './Ganancias';
import MargenPlatos from './MargenPlatos';
import VerComprobanteVenta from '../../components/VerComprobanteVenta';

// Nombres de la tabla "Ventas por método de pago" que piden aclaración.
const NOMBRE_EN_REPORTE = {
  YAPE_PLIN: 'Yape/Plin (ventas de antes, sin separar)',
  CREDITO: 'Al crédito (por cobrar)',
};

// El día de hoy en Perú (el servidor corta los reportes en esa hora).
const hoy = () => hoyLima();

const RANGOS_RAPIDOS = [
  { label: 'Hoy', dias: 0 },
  { label: '7 días', dias: 6 },
  { label: '30 días', dias: 29 },
];

const soles = (n) => `S/ ${(Number(n) || 0).toFixed(2)}`;

// Comprobante de una venta: "Boleta BT01-15" y su estado, o solo ticket.
const TIPO_COMPROBANTE = { BOLETA: 'Boleta', FACTURA: 'Factura' };
const ESTADO_COMPROBANTE = {
  ACEPTADO: ['Aceptado', 'ok'],
  PENDIENTE: ['Pendiente', 'aviso'],
  RECHAZADO: ['Rechazado', 'error'],
  ERROR: ['Con error', 'error'],
};
function datosComprobante(c) {
  if (!c) return { texto: 'Sin comprobante', estado: 'Solo ticket', tono: 'tenue' };
  const numero = c.serie && c.numero != null ? ` ${c.serie}-${c.numero}` : '';
  const [estado, tono] = ESTADO_COMPROBANTE[c.estado] || [c.estado || '', 'tenue'];
  return { texto: `${TIPO_COMPROBANTE[c.tipo] || c.tipo}${numero}`, estado, tono };
}

// Con qué comparar lo cobrado por cada medio (franja del filtro).
function ayudaDelMetodo(metodo) {
  if (metodo === 'EFECTIVO') return 'Compáralo con el efectivo de la caja.';
  if (metodo === 'CREDITO') return 'Es lo que quedó por cobrar.';
  return `Compáralo con los movimientos de ${nombreMetodo(metodo)} a esas horas.`;
}

// ganancias: el negocio tiene encendido el módulo "Reporte de ganancias" y
// quien mira es el administrador. Sin eso, esta pantalla es la de siempre.
// metodoInicial: abrir con las ventas ya filtradas por ese medio (desde el
// Resumen); onFiltroUsado avisa que ya se aplicó, para no repetirlo.
// costeo: restaurante con el módulo "Margen por plato" (solo administrador).
export default function Reportes({ ganancias = false, costeo = false, onVerSinPrecio, metodoInicial = null, onFiltroUsado }) {
  const [pestana, setPestana] = useState('VENTAS');
  const [fechaInicio, setFechaInicio] = useState(hoy());
  const [fechaFin, setFechaFin] = useState(hoy());
  const [estadisticas, setEstadisticas] = useState(null);
  const [productosVendidos, setProductosVendidos] = useState([]);
  const [ventas, setVentas] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [mensaje, setMensaje] = useState(null);
  // Filtros de "Ventas del período" y la venta con sus productos abiertos.
  const [metodoFiltro, setMetodoFiltro] = useState(metodoInicial || 'TODOS');
  const [cajeroFiltro, setCajeroFiltro] = useState('TODOS');
  const [ventaAbierta, setVentaAbierta] = useState(null);
  // Venta cuyo comprobante se está viendo (solo lectura).
  const [comprobanteDe, setComprobanteDe] = useState(null);
  const seccionVentas = useRef(null);

  useEffect(() => {
    if (metodoInicial) onFiltroUsado?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const cargar = () => {
    setCargando(true);
    setMensaje(null);
    Promise.all([
      api.reportesEstadisticas(fechaInicio, fechaFin),
      api.reportesProductosVendidos(fechaInicio, fechaFin),
      api.reportesVentas(fechaInicio, fechaFin),
    ])
      .then(([e, p, v]) => {
        setEstadisticas(e);
        setProductosVendidos(p);
        setVentas(v);
      })
      .catch((err) => setMensaje({ tipo: 'error', texto: err.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fechaInicio, fechaFin]);

  // Ventas que tocan el medio elegido (un mixto aparece en sus dos medios)
  // y lo que entró por ese medio en cada una.
  const metodos = ['TODOS', ...new Set([...(estadisticas?.por_metodo || []).map((m) => m.metodo), metodoFiltro])].filter(
    (m, i, lista) => lista.indexOf(m) === i
  );
  const cajeros = [...new Set(ventas.map((v) => v.cajero).filter(Boolean))].sort();
  const cobradoDe = (v) =>
    metodoFiltro === 'TODOS'
      ? Number(v.total) || 0
      : partesDelPago(v)
          .filter((p) => p.metodo === metodoFiltro)
          .reduce((suma, p) => suma + p.monto, 0);
  const ventasVisibles = ventas.filter(
    (v) =>
      (metodoFiltro === 'TODOS' || partesDelPago(v).some((p) => p.metodo === metodoFiltro)) &&
      (cajeroFiltro === 'TODOS' || v.cajero === cajeroFiltro)
  );
  // Los totales cuentan solo ventas completadas (igual que la tabla de arriba).
  const completadas = ventasVisibles.filter((v) => v.estado === 'COMPLETADA');
  const totalCobrado = completadas.reduce((suma, v) => suma + cobradoDe(v), 0);
  const nombreFiltro = metodoFiltro === 'TODOS' ? '' : nombreMetodo(metodoFiltro);
  const deCajero = cajeroFiltro === 'TODOS' ? '' : ` · ${cajeroFiltro}`;

  const verVentasDe = (metodo) => {
    setMetodoFiltro((actual) => (actual === metodo ? 'TODOS' : metodo));
    seccionVentas.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };
  const alternarVenta = (id) => setVentaAbierta((actual) => (actual === id ? null : id));

  const aplicarRangoRapido = (dias) => {
    setFechaInicio(hoyLima(-dias));
    setFechaFin(hoyLima());
  };

  const pestanas = (ganancias || costeo) && (
    <div className="rep-pestanas" role="tablist" aria-label="Tipo de reporte">
      {[
        ['VENTAS', 'Ventas'],
        ganancias && ['GANANCIAS', 'Ganancias'],
        costeo && ['MARGEN', 'Margen por plato'],
      ].filter(Boolean).map(([valor, texto]) => (
        <button
          key={valor}
          type="button"
          role="tab"
          aria-selected={pestana === valor}
          className={pestana === valor ? 'activa' : ''}
          onClick={() => setPestana(valor)}
        >
          {texto}
        </button>
      ))}
    </div>
  );

  if (costeo && pestana === 'MARGEN') {
    return (
      <div className="rep-layout">
        <div className="rep-header">
          <h1>Reportes</h1>
        </div>
        {pestanas}
        <MargenPlatos />
      </div>
    );
  }

  if (ganancias && pestana === 'GANANCIAS') {
    return (
      <div className="rep-layout">
        <div className="rep-header">
          <h1>Reportes</h1>
        </div>
        {pestanas}
        <Ganancias onVerSinPrecio={onVerSinPrecio} />
      </div>
    );
  }

  return (
    <div className="rep-layout">
      <div className="rep-header">
        <h1>Reportes</h1>
        <div className="rep-rangos-rapidos">
          {RANGOS_RAPIDOS.map((r) => (
            <button key={r.label} onClick={() => aplicarRangoRapido(r.dias)}>
              {r.label}
            </button>
          ))}
        </div>
      </div>

      {pestanas}

      <div className="rep-fechas">
        <div className="rep-campo-fecha">
          <label>Desde</label>
          <input type="date" value={fechaInicio} onChange={(e) => setFechaInicio(e.target.value)} max={fechaFin} />
        </div>
        <div className="rep-campo-fecha">
          <label>Hasta</label>
          <input type="date" value={fechaFin} onChange={(e) => setFechaFin(e.target.value)} min={fechaInicio} max={hoy()} />
        </div>
      </div>

      {mensaje && <p className="rep-mensaje">{mensaje.texto}</p>}

      {cargando ? (
        <p className="rep-cargando">Cargando...</p>
      ) : (
        <>
          {estadisticas && (
            <div className="rep-tarjetas">
              <div className="rep-tarjeta">
                <span className="rep-tarjeta-label">Ventas</span>
                <strong className="rep-tarjeta-valor">{estadisticas.ventas_cantidad}</strong>
                <span className="rep-tarjeta-sub">S/ {estadisticas.ventas_total.toFixed(2)}</span>
              </div>
              <div className="rep-tarjeta">
                <span className="rep-tarjeta-label">Ticket promedio</span>
                <strong className="rep-tarjeta-valor">S/ {estadisticas.ticket_promedio.toFixed(2)}</strong>
              </div>
              <div className="rep-tarjeta rep-tarjeta-negativa">
                <span className="rep-tarjeta-label">Devoluciones</span>
                <strong className="rep-tarjeta-valor">{estadisticas.devoluciones_cantidad}</strong>
                <span className="rep-tarjeta-sub">- S/ {estadisticas.devoluciones_total.toFixed(2)}</span>
              </div>
              {/* Gastos del negocio en las fechas elegidas (solo lo ve el administrador). */}
              {estadisticas.gastos_total != null && (
                <div className="rep-tarjeta rep-tarjeta-negativa">
                  <span className="rep-tarjeta-label">Gastos</span>
                  <strong className="rep-tarjeta-valor">{estadisticas.gastos_cantidad}</strong>
                  <span className="rep-tarjeta-sub">- S/ {estadisticas.gastos_total.toFixed(2)}</span>
                </div>
              )}
              <div className="rep-tarjeta rep-tarjeta-destacada">
                <span className="rep-tarjeta-label">Total neto</span>
                <strong className="rep-tarjeta-valor">S/ {estadisticas.total_neto.toFixed(2)}</strong>
              </div>
            </div>
          )}

          {estadisticas?.por_metodo?.length > 0 && (
            <div className="rep-seccion">
              <h2>Ventas por método de pago</h2>
              <div className="rep-tabla-wrapper">
                <table className="rep-tabla rep-tabla-metodos">
                  <thead>
                    <tr>
                      <th>Método</th>
                      <th className="num">Pagos</th>
                      <th className="num">Monto</th>
                      <th className="rep-metodo-barra-th">Del total</th>
                      <th>
                        <span className="rep-solo-lector">Ver ventas</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {estadisticas.por_metodo.map((m) => {
                      const parte = estadisticas.ventas_total > 0 ? (m.monto / estadisticas.ventas_total) * 100 : 0;
                      return (
                        <tr key={m.metodo} className={metodoFiltro === m.metodo ? 'rep-fila-activa' : ''}>
                          <td>{NOMBRE_EN_REPORTE[m.metodo] || nombreMetodo(m.metodo)}</td>
                          <td className="num">{m.cantidad}</td>
                          <td className="num">S/ {m.monto.toFixed(2)}</td>
                          <td>
                            <div className="rep-metodo-barra">
                              <span className="rep-metodo-pista">
                                <span className="rep-metodo-relleno" style={{ width: `${Math.min(100, parte)}%` }} />
                              </span>
                              <span className="rep-metodo-porcentaje">{parte.toFixed(1)}%</span>
                            </div>
                          </td>
                          <td className="rep-celda-accion">
                            <button
                              type="button"
                              className={`rep-boton-ver${metodoFiltro === m.metodo ? ' activo' : ''}`}
                              aria-pressed={metodoFiltro === m.metodo}
                              onClick={() => verVentasDe(m.metodo)}
                            >
                              {metodoFiltro === m.metodo ? 'Viendo' : 'Ver ventas'}
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
              <p className="rep-nota">
                Una venta con pago mixto cuenta su parte en efectivo y su parte en el otro medio. No descuenta las
                devoluciones.
              </p>
            </div>
          )}

          <div className="rep-seccion">
            <h2>Productos más vendidos</h2>
            <div className="rep-tabla-wrapper">
              <table className="rep-tabla">
                <thead>
                  <tr>
                    <th>Producto</th>
                    <th>Cantidad</th>
                    <th>Total vendido</th>
                  </tr>
                </thead>
                <tbody>
                  {productosVendidos.map((p, idx) => (
                    <tr key={idx}>
                      <td>{p.producto_nombre}</td>
                      <td>{p.cantidad_vendida}</td>
                      <td>S/ {p.total_vendido.toFixed(2)}</td>
                    </tr>
                  ))}
                  {productosVendidos.length === 0 && (
                    <tr>
                      <td colSpan={3} className="rep-sin-resultados">
                        No hay ventas en este período.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>

          <div className="rep-seccion" ref={seccionVentas}>
            <h2>Ventas del período ({ventasVisibles.length})</h2>

            <div className="rep-filtros">
              <div className="rep-filtro" role="group" aria-label="Filtrar por método de pago">
                <span className="rep-filtro-titulo">Método</span>
                {metodos.map((m) => (
                  <button
                    key={m}
                    type="button"
                    className={`rep-chip${metodoFiltro === m ? ' activo' : ''}`}
                    aria-pressed={metodoFiltro === m}
                    onClick={() => setMetodoFiltro(m)}
                  >
                    {m === 'TODOS' ? 'Todos' : nombreMetodo(m)}
                  </button>
                ))}
              </div>
              {cajeros.length > 1 && (
                <div className="rep-filtro" role="group" aria-label="Filtrar por cajero">
                  <span className="rep-filtro-titulo">Cajero</span>
                  {['TODOS', ...cajeros].map((c) => (
                    <button
                      key={c}
                      type="button"
                      className={`rep-chip${cajeroFiltro === c ? ' activo' : ''}`}
                      aria-pressed={cajeroFiltro === c}
                      onClick={() => setCajeroFiltro(c)}
                    >
                      {c === 'TODOS' ? 'Todos' : c}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {metodoFiltro !== 'TODOS' && (
              <div className="rep-franja">
                <strong>
                  Cobrado por {nombreFiltro}: {completadas.length} {completadas.length === 1 ? 'pago' : 'pagos'} ·{' '}
                  {soles(totalCobrado)}
                  {deCajero}
                </strong>
                <span>{ayudaDelMetodo(metodoFiltro)}</span>
              </div>
            )}

            <div className="rep-tabla-wrapper rep-ventas-tabla">
              <table className="rep-tabla">
                <thead>
                  <tr>
                    <th>Folio</th>
                    <th>Fecha</th>
                    <th>Cajero</th>
                    <th>Cómo pagó</th>
                    <th>Comprobante</th>
                    <th>Estado</th>
                    {metodoFiltro !== 'TODOS' && <th className="num">Por {nombreFiltro}</th>}
                    <th className="num">Total</th>
                    <th>
                      <span className="rep-solo-lector">Productos</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {ventasVisibles.map((v) => {
                    const partes = partesDelPago(v);
                    const comp = datosComprobante(v.comprobante);
                    const abierta = ventaAbierta === v.id;
                    return (
                      <FilaVenta
                        key={v.id}
                        venta={v}
                        partes={partes}
                        comp={comp}
                        abierta={abierta}
                        metodoFiltro={metodoFiltro}
                        cobrado={cobradoDe(v)}
                        onAlternar={() => alternarVenta(v.id)}
                        onVerComprobante={() => setComprobanteDe(v)}
                      />
                    );
                  })}
                  {ventasVisibles.length === 0 && (
                    <tr>
                      <td colSpan={9} className="rep-sin-resultados">
                        {ventas.length === 0 ? 'No hay ventas en este período.' : 'Ninguna venta coincide con el filtro.'}
                      </td>
                    </tr>
                  )}
                </tbody>
                {ventasVisibles.length > 0 && (
                  <tfoot>
                    <tr>
                      <td colSpan={6} className="rep-pie-texto">
                        {metodoFiltro === 'TODOS'
                          ? `Total de ${completadas.length} ventas completadas${deCajero}`
                          : `Total cobrado por ${nombreFiltro}${deCajero}`}
                      </td>
                      {metodoFiltro !== 'TODOS' && <td className="num rep-pie-total">{soles(totalCobrado)}</td>}
                      <td className={`num${metodoFiltro === 'TODOS' ? ' rep-pie-total' : ''}`}>
                        {metodoFiltro === 'TODOS' ? soles(totalCobrado) : ''}
                      </td>
                      <td />
                    </tr>
                  </tfoot>
                )}
              </table>
            </div>

            <div className="rep-ventas-tarjetas">
              {ventasVisibles.map((v) => {
                const comp = datosComprobante(v.comprobante);
                const abierta = ventaAbierta === v.id;
                return (
                  <div key={v.id} className="rep-venta-tarjeta">
                    <button type="button" className="rep-venta-cabeza" aria-expanded={abierta} onClick={() => alternarVenta(v.id)}>
                      <span className="rep-venta-datos">
                        <strong>
                          {fechaHoraLima(v.fecha_hora)} · {v.cajero}
                        </strong>
                        <small>
                          <ComoPago partes={partesDelPago(v)} metodoFiltro={metodoFiltro} />
                        </small>
                        <small className={`rep-comp-${comp.tono}`}>
                          {comp.texto} · {comp.estado}
                          {v.estado !== 'COMPLETADA' ? ` · Venta ${v.estado.toLowerCase()}` : ''}
                        </small>
                      </span>
                      <strong className="rep-venta-monto">{soles(cobradoDe(v))}</strong>
                    </button>
                    {abierta && (
                      <ProductosDeVenta
                        venta={v}
                        esMixto={partesDelPago(v).length > 1}
                        onVerComprobante={() => setComprobanteDe(v)}
                      />
                    )}
                  </div>
                );
              })}
              {ventasVisibles.length === 0 && (
                <p className="rep-sin-resultados">
                  {ventas.length === 0 ? 'No hay ventas en este período.' : 'Ninguna venta coincide con el filtro.'}
                </p>
              )}
            </div>
            {comprobanteDe && <VerComprobanteVenta venta={comprobanteDe} onCerrar={() => setComprobanteDe(null)} />}
            <p className="rep-nota">
              Los totales cuentan solo ventas completadas. En un pago mixto, cada medio cuenta solo su parte; los productos son de
              toda la venta.
            </p>
          </div>
        </>
      )}
    </div>
  );
}
/** "Cómo pagó": cada parte del pago; con un filtro, resalta la de ese medio. */
function ComoPago({ partes, metodoFiltro }) {
  const mixto = partes.length > 1;
  return (
    <span className="rep-como-pago">
      {mixto && <span className="rep-etiqueta-mixto">Mixto</span>}
      {partes.map((p, i) => {
        const tono = metodoFiltro === 'TODOS' ? '' : p.metodo === metodoFiltro ? ' resaltada' : ' atenuada';
        return (
          <span key={i} className={`rep-pieza${tono}`}>
            {mixto ? `${soles(p.monto)} ` : ''}
            {nombreMetodo(p.metodo)}
            {i < partes.length - 1 ? ' +' : ''}
          </span>
        );
      })}
    </span>
  );
}

// Texto del botón según el comprobante que tiene la venta.
function textoVerComprobante(venta) {
  const tipo = venta.comprobante?.tipo;
  if (tipo === 'FACTURA') return 'Ver factura';
  if (tipo === 'BOLETA') return 'Ver boleta';
  return 'Ver nota de venta';
}

/** Los productos de una venta, al desplegarla. */
function ProductosDeVenta({ venta, esMixto, onVerComprobante }) {
  const productos = venta.productos || [];
  return (
    <div className="rep-productos">
      {venta.estado === 'COMPLETADA' && onVerComprobante && (
        <div className="rep-productos-acciones">
          <button type="button" className="rep-boton-ver" onClick={onVerComprobante}>
            {textoVerComprobante(venta)}
          </button>
        </div>
      )}
      {productos.length === 0 ? (
        <p className="rep-nota">No se pudieron leer los productos de esta venta.</p>
      ) : (
        <table className="rep-productos-tabla">
          <thead>
            <tr>
              <th>Producto</th>
              <th className="num">Cant.</th>
              <th className="num">Precio</th>
              <th className="num">Subtotal</th>
            </tr>
          </thead>
          <tbody>
            {productos.map((p, i) => (
              <tr key={i}>
                <td>{p.nombre}</td>
                <td className="num">{Number(p.cantidad).toLocaleString('es-PE', { maximumFractionDigits: 3 })}</td>
                <td className="num">{soles(p.precio_unitario)}</td>
                <td className="num">{soles(p.subtotal)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {esMixto && <p className="rep-nota">Pago mixto: los productos son de toda la venta; el pago se repartió como dice "Cómo pagó".</p>}
    </div>
  );
}

function FilaVenta({ venta, partes, comp, abierta, metodoFiltro, cobrado, onAlternar, onVerComprobante }) {
  const columnas = metodoFiltro === 'TODOS' ? 8 : 9;
  return (
    <>
      <tr className={abierta ? 'rep-fila-abierta' : ''}>
        <td>{venta.folio}</td>
        <td>{fechaHoraLima(venta.fecha_hora)}</td>
        <td>{venta.cajero}</td>
        <td>
          <ComoPago partes={partes} metodoFiltro={metodoFiltro} />
        </td>
        <td>
          <span className="rep-comp">
            <span>{comp.texto}</span>
            <small className={`rep-comp-${comp.tono}`}>{comp.estado}</small>
          </span>
        </td>
        <td>
          <span className={`rep-badge rep-badge-${venta.estado.toLowerCase()}`}>{venta.estado}</span>
        </td>
        {metodoFiltro !== 'TODOS' && <td className="num rep-cobrado">{soles(cobrado)}</td>}
        <td className={`num${metodoFiltro === 'TODOS' ? ' rep-cobrado' : ' rep-total-tenue'}`}>{soles(venta.total)}</td>
        <td className="rep-celda-accion">
          <button type="button" className={`rep-boton-ver${abierta ? ' activo' : ''}`} aria-expanded={abierta} onClick={onAlternar}>
            {abierta ? 'Ocultar' : `Productos (${(venta.productos || []).length})`}
          </button>
        </td>
      </tr>
      {abierta && (
        <tr className="rep-fila-detalle">
          <td colSpan={columnas}>
            <ProductosDeVenta venta={venta} esMixto={partes.length > 1} onVerComprobante={onVerComprobante} />
          </td>
        </tr>
      )}
    </>
  );
}
