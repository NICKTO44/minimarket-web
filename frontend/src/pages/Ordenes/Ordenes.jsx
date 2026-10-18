import { useCallback, useEffect, useMemo, useState } from 'react';
import { ClipboardList, Package, Plus, Printer, ShoppingCart, Users, Wrench } from 'lucide-react';
import { api } from '../../api/api';
import { fechaCorta, hoyLima } from '../../utils/formato';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import { formatoPlaca, hoyMasMeses, lineasDePaquete, nombreVehiculo, textoKm } from '../../utils/vehiculos';
import '../../components/PantallaModulo.css';
import '../../components/Recibo.css';

const FILTROS = [
  { valor: 'ABIERTAS', label: 'En taller' },
  { valor: 'COBRADAS', label: 'Cobradas' },
  { valor: 'TODAS', label: 'Todas' },
];

const ETIQUETA_ESTADO = {
  ABIERTA: ['En trabajo', 'pm-chip-aviso'],
  TERMINADA: ['Lista para cobrar', 'pm-chip-ok'],
  COBRADA: ['Cobrada', ''],
  ANULADA: ['Anulada', 'pm-chip-mal'],
};

function Estado({ estado }) {
  const [texto, clase] = ETIQUETA_ESTADO[estado] || [estado, ''];
  return <span className={`pm-chip ${clase}`}>{texto}</span>;
}

/** Lo que se manda al servidor de cada línea. */
const paraGuardar = (items) =>
  items.map((i) => ({ producto_id: i.producto_id, cantidad: i.cantidad, descuento: i.descuento || 0, paquete_id: i.paquete_id || null }));

/** La orden para la ticketera (como la cotización: solo se ve al imprimir). */
function OrdenImprimible({ orden, nombreTienda, direccion, telefono }) {
  if (!orden) return null;
  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">ORDEN DE SERVICIO</div>
      <div className="recibo-centro recibo-comprobante-numero">N° {orden.numero}</div>
      <div className="recibo-linea"></div>
      <div className="recibo-fila-meta">
        <span>Placa</span>
        <span>{orden.placa}</span>
      </div>
      {nombreVehiculo(orden) && (
        <div className="recibo-fila-meta">
          <span>Vehículo</span>
          <span>{nombreVehiculo(orden)}</span>
        </div>
      )}
      {orden.km != null && (
        <div className="recibo-fila-meta">
          <span>Kilometraje</span>
          <span>{textoKm(orden.km)}</span>
        </div>
      )}
      {orden.cliente_nombre && (
        <div className="recibo-fila-meta">
          <span>Cliente</span>
          <span>{orden.cliente_nombre}</span>
        </div>
      )}
      {orden.mecanico_nombre && (
        <div className="recibo-fila-meta">
          <span>Mecánico</span>
          <span>{orden.mecanico_nombre}</span>
        </div>
      )}
      <div className="recibo-fila-meta">
        <span>Ingreso</span>
        <span>{fechaCorta(orden.fecha_creacion)}</span>
      </div>
      {orden.observaciones && <div className="recibo-cliente-nombre">{orden.observaciones}</div>}
      <div className="recibo-linea"></div>
      {(orden.items || []).map((i) => (
        <div key={i.id} className="recibo-item">
          <div className="recibo-item-nombre">{i.nombre}</div>
          <div className="recibo-item-detalle">
            <span>
              {formatoCantidad(i.cantidad)} x S/.{i.precio_unitario.toFixed(2)}
            </span>
            <span>S/.{i.total.toFixed(2)}</span>
          </div>
        </div>
      ))}
      <div className="recibo-linea-doble"></div>
      <div className="recibo-total">
        <span>TOTAL</span>
        <span>S/.{orden.total.toFixed(2)}</span>
      </div>
      <div className="recibo-centro recibo-disclaimer">Este documento no es un comprobante de pago.</div>
    </div>
  );
}

/**
 * Órdenes de servicio (módulo ORDENES_SERVICIO, lubricentro). El vehículo
 * entra al taller, se le van agregando repuestos y mano de obra, y al
 * terminar se cobra en el punto de venta. No toca el stock ni la caja hasta
 * que se cobra.
 */
export default function Ordenes({ esAdmin = false, conPaquetes = false, nombreTienda, direccion, telefono, onCobrar }) {
  const [vista, setVista] = useState('ORDENES');
  const [filtro, setFiltro] = useState('ABIERTAS');
  const [lista, setLista] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [mecanicos, setMecanicos] = useState([]);
  const [abierta, setAbierta] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const [nueva, setNueva] = useState(null);

  const cargar = useCallback(() => {
    setCargando(true);
    api
      .ordenes(filtro)
      .then((d) => {
        setLista(d);
        setError(null);
      })
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  }, [filtro]);
  const cargarMecanicos = useCallback(() => api.mecanicos().then(setMecanicos).catch(() => {}), []);

  useEffect(() => {
    if (vista === 'ORDENES') cargar();
  }, [cargar, vista]);
  useEffect(() => {
    cargarMecanicos();
  }, [cargarMecanicos]);

  const abrir = async (id) => {
    setError(null);
    try {
      setAbierta(await api.orden(id));
    } catch (e) {
      setError(e.message);
    }
  };

  const crear = async () => {
    const placa = formatoPlaca(nueva.placa);
    if (!placa) {
      setNueva((n) => ({ ...n, error: 'Revisa la placa (por ejemplo ABC-123).' }));
      return;
    }
    setOcupado(true);
    try {
      const orden = await api.ordenCrear({
        placa,
        marca: nueva.marca.trim() || null,
        modelo: nueva.modelo.trim() || null,
        km: nueva.km ? Number(nueva.km) : null,
        mecanico_id: nueva.mecanico_id ? Number(nueva.mecanico_id) : null,
        observaciones: nueva.observaciones.trim() || null,
      });
      setNueva(null);
      setFiltro('ABIERTAS');
      cargar();
      setAbierta(orden);
    } catch (e) {
      setNueva((n) => ({ ...n, error: e.message }));
    } finally {
      setOcupado(false);
    }
  };

  const activos = mecanicos.filter((m) => m.activo);

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <ClipboardList size={22} /> Órdenes de servicio
          </h1>
          <p className="pm-subtitulo">El vehículo entra al taller y se va armando su orden. El stock se descuenta recién al cobrarla.</p>
        </div>
        <div className="pm-acciones">
          <div className="pm-filtros" role="tablist">
            {[
              ['ORDENES', 'Órdenes'],
              ...(esAdmin
                ? [
                    ['COMISIONES', 'Comisiones'],
                    ['MECANICOS', 'Mecánicos'],
                  ]
                : []),
            ].map(([valor, label]) => (
              <button key={valor} role="tab" aria-selected={vista === valor} className={vista === valor ? 'activo' : ''} onClick={() => setVista(valor)}>
                {label}
              </button>
            ))}
          </div>
          <button className="pm-boton" onClick={() => setNueva({ placa: '', marca: '', modelo: '', km: '', mecanico_id: '', observaciones: '', vehiculo: null })}>
            <Plus size={15} /> Nueva orden
          </button>
        </div>
      </header>

      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      {vista === 'ORDENES' && (
        <>
          <div className="pm-filtros" role="tablist" style={{ marginBottom: 12 }}>
            {FILTROS.map((f) => (
              <button key={f.valor} role="tab" aria-selected={filtro === f.valor} className={filtro === f.valor ? 'activo' : ''} onClick={() => setFiltro(f.valor)}>
                {f.label}
              </button>
            ))}
          </div>
          <div className="pm-tarjeta">
            {cargando ? (
              <p className="pm-vacio">Cargando...</p>
            ) : lista.length === 0 ? (
              <p className="pm-vacio">{filtro === 'ABIERTAS' ? 'No hay vehículos en el taller. Toca “Nueva orden” cuando entre uno.' : 'No hay órdenes.'}</p>
            ) : (
              lista.map((o) => (
                <button key={o.id} className="pm-fila" onClick={() => abrir(o.id)}>
                  <span className="pm-fila-principal">
                    <span className="pm-fila-titulo">
                      N° {o.numero} · {o.placa} {nombreVehiculo(o) && `· ${nombreVehiculo(o)}`} <Estado estado={o.estado} />
                    </span>
                    <span className="pm-fila-detalle" style={{ display: 'block' }}>
                      {fechaCorta(o.fecha_creacion)} · {o.cliente_nombre || 'Sin cliente'} · {o.mecanico_nombre || 'Sin mecánico'} · {o.items_cantidad}{' '}
                      {o.items_cantidad === 1 ? 'línea' : 'líneas'}
                      {o.venta_folio ? ` · venta ${o.venta_folio}` : ''}
                    </span>
                  </span>
                  <span className="pm-fila-monto">
                    <strong>S/ {o.total.toFixed(2)}</strong>
                  </span>
                </button>
              ))
            )}
          </div>
        </>
      )}

      {vista === 'COMISIONES' && esAdmin && <Comisiones />}
      {vista === 'MECANICOS' && esAdmin && <Mecanicos mecanicos={mecanicos} onCambio={cargarMecanicos} />}

      {nueva && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setNueva(null)}>
          <div className="pm-modal" role="dialog" aria-modal="true" aria-label="Nueva orden">
            <div className="pm-modal-cabecera">
              <h2>Nueva orden de servicio</h2>
              <p>Si la placa ya está registrada, se usan su dueño y sus datos.</p>
            </div>
            <div className="pm-modal-cuerpo">
              {nueva.error && <p className="pm-mensaje pm-mensaje-error">{nueva.error}</p>}
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Placa</span>
                  <input
                    autoFocus
                    value={nueva.placa}
                    maxLength={10}
                    onChange={(e) => setNueva((n) => ({ ...n, placa: e.target.value.toUpperCase(), vehiculo: null }))}
                    onBlur={async () => {
                      const placa = formatoPlaca(nueva.placa);
                      if (!placa) return;
                      try {
                        const v = await api.vehiculoPorPlaca(placa);
                        setNueva((n) => ({ ...n, placa: v.placa, vehiculo: v, km: n.km || '' }));
                      } catch {
                        setNueva((n) => ({ ...n, placa, vehiculo: false }));
                      }
                    }}
                  />
                  {nueva.vehiculo && (
                    <small>
                      {nombreVehiculo(nueva.vehiculo) || 'Registrado'} · {nueva.vehiculo.cliente_nombre || 'sin dueño'}
                      {nueva.vehiculo.km_ultimo != null && ` · último: ${textoKm(nueva.vehiculo.km_ultimo)}`}
                    </small>
                  )}
                  {nueva.vehiculo === false && <small>Vehículo nuevo: se registra con la orden.</small>}
                </label>
                <label className="pm-campo">
                  <span>Kilometraje de hoy</span>
                  <input inputMode="numeric" value={nueva.km} onChange={(e) => setNueva((n) => ({ ...n, km: e.target.value.replace(/[^\d]/g, '') }))} />
                </label>
                {nueva.vehiculo === false && (
                  <>
                    <label className="pm-campo">
                      <span>Marca</span>
                      <input value={nueva.marca} maxLength={40} onChange={(e) => setNueva((n) => ({ ...n, marca: e.target.value }))} />
                    </label>
                    <label className="pm-campo">
                      <span>Modelo</span>
                      <input value={nueva.modelo} maxLength={60} onChange={(e) => setNueva((n) => ({ ...n, modelo: e.target.value }))} />
                    </label>
                  </>
                )}
                <label className="pm-campo pm-campo-ancho">
                  <span>Mecánico</span>
                  <select value={nueva.mecanico_id} onChange={(e) => setNueva((n) => ({ ...n, mecanico_id: e.target.value }))}>
                    <option value="">Sin asignar</option>
                    {activos.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.nombre}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="pm-campo pm-campo-ancho">
                  <span>Qué pide el cliente</span>
                  <textarea
                    rows={2}
                    maxLength={500}
                    value={nueva.observaciones}
                    placeholder="Cambio de aceite y filtro, revisar frenos…"
                    onChange={(e) => setNueva((n) => ({ ...n, observaciones: e.target.value }))}
                  />
                </label>
              </div>
            </div>
            <div className="pm-modal-pie">
              <button className="pm-boton-secundario" onClick={() => setNueva(null)}>
                Cancelar
              </button>
              <button className="pm-boton" onClick={crear} disabled={ocupado}>
                Abrir orden
              </button>
            </div>
          </div>
        </div>
      )}

      {abierta && (
        <DetalleOrden
          orden={abierta}
          mecanicos={activos}
          conPaquetes={conPaquetes}
          onCambio={(o) => {
            setAbierta(o);
            cargar();
          }}
          onCerrar={() => setAbierta(null)}
          onCobrar={onCobrar}
        />
      )}

      <OrdenImprimible orden={abierta} nombreTienda={nombreTienda} direccion={direccion} telefono={telefono} />
    </div>
  );
}

/** El detalle de una orden: datos, lo que lleva y sus acciones. */
function DetalleOrden({ orden, mecanicos, conPaquetes, onCambio, onCerrar, onCobrar }) {
  const editable = orden.estado === 'ABIERTA' || orden.estado === 'TERMINADA';
  const [datos, setDatos] = useState(() => ({
    mecanico_id: orden.mecanico_id ? String(orden.mecanico_id) : '',
    km: orden.km != null ? String(orden.km) : '',
    proximo_km: orden.proximo_km != null ? String(orden.proximo_km) : '',
    proxima_fecha: orden.proxima_fecha || '',
    observaciones: orden.observaciones || '',
  }));
  const [productos, setProductos] = useState([]);
  const [paquetes, setPaquetes] = useState([]);
  const [buscar, setBuscar] = useState('');
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const datosCambiaron =
    datos.mecanico_id !== (orden.mecanico_id ? String(orden.mecanico_id) : '') ||
    datos.km !== (orden.km != null ? String(orden.km) : '') ||
    datos.proximo_km !== (orden.proximo_km != null ? String(orden.proximo_km) : '') ||
    datos.proxima_fecha !== (orden.proxima_fecha || '') ||
    datos.observaciones !== (orden.observaciones || '');

  useEffect(() => {
    if (!editable) return;
    api.productos().then(setProductos).catch(() => {});
    if (conPaquetes) api.paquetes().then(setPaquetes).catch(() => {});
  }, [editable, conPaquetes]);

  const encontrados = useMemo(() => {
    const q = buscar.trim().toLowerCase();
    if (q.length < 2) return [];
    return productos.filter((p) => p.nombre.toLowerCase().includes(q) || String(p.codigo).toLowerCase().includes(q)).slice(0, 8);
  }, [productos, buscar]);

  const ejecutar = async (accion) => {
    setOcupado(true);
    setError(null);
    try {
      onCambio(await accion());
      return true;
    } catch (e) {
      setError(e.message);
      return false;
    } finally {
      setOcupado(false);
    }
  };

  const guardarItems = (items) => ejecutar(() => api.ordenItems(orden.id, paraGuardar(items)));
  const items = orden.items || [];

  const agregarProducto = (p) => {
    setBuscar('');
    const existe = items.find((i) => i.producto_id === p.id && !i.paquete_id);
    const nuevos = existe
      ? items.map((i) => (i === existe ? { ...i, cantidad: i.cantidad + 1 } : i))
      : [...items, { producto_id: p.id, cantidad: 1, descuento: 0 }];
    guardarItems(nuevos);
  };
  const agregarPaquete = (paquete) => {
    const lineas = lineasDePaquete(paquete).map((l) => ({ producto_id: l.producto_id, cantidad: l.cantidad, descuento: l.descuento, paquete_id: paquete.id }));
    guardarItems([...items, ...lineas]);
  };
  const cambiarCantidad = (item, texto) => {
    const cantidad = Number(String(texto).replace(',', '.'));
    if (!(cantidad > 0) || cantidad === item.cantidad) return;
    guardarItems(items.map((i) => (i.id === item.id ? { ...i, cantidad } : i)));
  };
  const quitar = (item) =>
    guardarItems(item.paquete_id ? items.filter((i) => i.paquete_id !== item.paquete_id) : items.filter((i) => i.id !== item.id));

  const guardarDatos = () =>
    ejecutar(() =>
      api.ordenActualizar(orden.id, {
        mecanico_id: datos.mecanico_id ? Number(datos.mecanico_id) : null,
        cliente_id: orden.cliente_id || null,
        km: datos.km ? Number(datos.km) : null,
        proximo_km: datos.proximo_km ? Number(datos.proximo_km) : null,
        proxima_fecha: datos.proxima_fecha || null,
        observaciones: datos.observaciones.trim() || null,
      })
    );

  const anular = async () => {
    const motivo = window.prompt(`¿Por qué se anula la orden N° ${orden.numero}?`);
    if (!motivo?.trim()) return;
    ejecutar(() => api.ordenEstado(orden.id, 'ANULADA', motivo.trim()));
  };

  const cobrar = async () => {
    if (datosCambiaron && !(await guardarDatos())) return;
    const sinStock = items.filter((i) => i.controla_stock && i.stock + 1e-6 < i.cantidad);
    if (sinStock.length > 0) {
      setError(`No alcanza el stock de: ${sinStock.map((i) => i.nombre).join(', ')}. Ingresa la compra o quita esas líneas antes de cobrar.`);
      return;
    }
    onCobrar(await api.orden(orden.id));
  };

  const kmHoy = Number(datos.km) || 0;

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label={`Orden ${orden.numero}`}>
        <div className="pm-modal-cabecera">
          <h2>
            Orden N° {orden.numero} · {orden.placa} <Estado estado={orden.estado} />
          </h2>
          <p>
            {nombreVehiculo(orden) || 'Vehículo'} · {orden.cliente_nombre || 'Sin cliente'} · ingresó el {fechaCorta(orden.fecha_creacion)}
            {orden.venta_folio ? ` · venta ${orden.venta_folio}` : ''}
          </p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Mecánico</span>
              <select disabled={!editable} value={datos.mecanico_id} onChange={(e) => setDatos((d) => ({ ...d, mecanico_id: e.target.value }))}>
                <option value="">Sin asignar</option>
                {mecanicos.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.nombre}
                  </option>
                ))}
                {orden.mecanico_id && !mecanicos.some((m) => m.id === orden.mecanico_id) && (
                  <option value={orden.mecanico_id}>{orden.mecanico_nombre}</option>
                )}
              </select>
            </label>
            <label className="pm-campo">
              <span>Kilometraje de hoy</span>
              <input
                disabled={!editable}
                inputMode="numeric"
                value={datos.km}
                onChange={(e) => setDatos((d) => ({ ...d, km: e.target.value.replace(/[^\d]/g, '') }))}
              />
            </label>
            <label className="pm-campo">
              <span>Próximo servicio (km)</span>
              <input
                disabled={!editable}
                inputMode="numeric"
                value={datos.proximo_km}
                placeholder={kmHoy ? String(kmHoy + 5000) : ''}
                onChange={(e) => setDatos((d) => ({ ...d, proximo_km: e.target.value.replace(/[^\d]/g, '') }))}
              />
            </label>
            <label className="pm-campo">
              <span>Próximo servicio (fecha)</span>
              <input
                disabled={!editable}
                type="date"
                min={hoyLima(1)}
                value={datos.proxima_fecha}
                onChange={(e) => setDatos((d) => ({ ...d, proxima_fecha: e.target.value }))}
              />
            </label>
            {editable && (
              <div className="pm-campo pm-campo-ancho">
                <div className="pm-opciones">
                  {kmHoy > 0 &&
                    [5000, 10000].map((n) => (
                      <button key={n} type="button" onClick={() => setDatos((d) => ({ ...d, proximo_km: String(kmHoy + n) }))}>
                        +{n.toLocaleString('es-PE')} km
                      </button>
                    ))}
                  {[3, 6].map((m) => (
                    <button key={m} type="button" onClick={() => setDatos((d) => ({ ...d, proxima_fecha: hoyMasMeses(m) }))}>
                      En {m} meses
                    </button>
                  ))}
                </div>
              </div>
            )}
            <label className="pm-campo pm-campo-ancho">
              <span>Observaciones</span>
              <textarea
                disabled={!editable}
                rows={2}
                maxLength={500}
                value={datos.observaciones}
                onChange={(e) => setDatos((d) => ({ ...d, observaciones: e.target.value }))}
              />
            </label>
          </div>
          {editable && datosCambiaron && (
            <button className="pm-boton-secundario" style={{ marginTop: 8 }} onClick={guardarDatos} disabled={ocupado}>
              Guardar datos
            </button>
          )}

          <p className="pm-seccion">Repuestos y mano de obra</p>
          {items.length === 0 ? (
            <p className="pm-vacio">Todavía no lleva nada. Busca abajo los repuestos y la mano de obra.</p>
          ) : (
            <table className="pm-lineas">
              <thead>
                <tr>
                  <th>Producto</th>
                  <th className="pm-num">Cantidad</th>
                  <th className="pm-num">Importe</th>
                  {editable && <th></th>}
                </tr>
              </thead>
              <tbody>
                {items.map((i) => (
                  <tr key={i.id}>
                    <td>
                      {i.nombre}
                      <small>
                        S/ {i.precio_unitario.toFixed(2)} c/u
                        {i.es_servicio ? ' · mano de obra' : ''}
                        {i.descuento > 0 ? ` · ${i.paquete_id ? 'paquete' : 'descuento'} −S/ ${i.descuento.toFixed(2)}` : ''}
                        {i.controla_stock && i.stock + 1e-6 < i.cantidad && editable ? ` · ⚠ stock: ${formatoCantidad(i.stock)}` : ''}
                      </small>
                    </td>
                    <td className="pm-num">
                      {editable && !i.paquete_id ? (
                        <input
                          key={`${i.id}-${i.cantidad}`}
                          defaultValue={formatoCantidad(i.cantidad)}
                          inputMode="decimal"
                          style={{ width: 64, textAlign: 'right' }}
                          aria-label={`Cantidad de ${i.nombre}`}
                          onBlur={(e) => cambiarCantidad(i, e.target.value)}
                          onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
                        />
                      ) : (
                        `${formatoCantidad(i.cantidad)} ${abreviaturaUnidad(i.unidad_medida)}`
                      )}
                    </td>
                    <td className="pm-num">S/ {i.total.toFixed(2)}</td>
                    {editable && (
                      <td className="pm-num">
                        <button
                          className="pm-boton-secundario"
                          onClick={() => quitar(i)}
                          disabled={ocupado}
                          title={i.paquete_id ? 'Quita el paquete completo' : 'Quitar'}
                          aria-label={`Quitar ${i.nombre}`}
                        >
                          ✕
                        </button>
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <div className="pm-total">
            <span>Total</span>
            <strong>S/ {orden.total.toFixed(2)}</strong>
          </div>

          {editable && (
            <>
              <div className="pm-campo" style={{ marginTop: 10 }}>
                <input value={buscar} placeholder="Agregar: busca el repuesto o la mano de obra por nombre o código" onChange={(e) => setBuscar(e.target.value)} />
              </div>
              {encontrados.map((p) => (
                <button key={p.id} className="pm-fila" onClick={() => agregarProducto(p)} disabled={ocupado}>
                  <span className="pm-fila-principal">
                    <span className="pm-fila-titulo">{p.nombre}</span>
                    <span className="pm-fila-detalle" style={{ display: 'block' }}>
                      {p.codigo} · {p.controla_stock === false ? 'servicio' : `stock ${formatoCantidad(p.stock)} ${abreviaturaUnidad(p.unidad_medida)}`}
                    </span>
                  </span>
                  <span className="pm-fila-monto">
                    <strong>S/ {p.precio.toFixed(2)}</strong>
                  </span>
                </button>
              ))}
              {conPaquetes && paquetes.length > 0 && (
                <div className="pm-opciones" style={{ marginTop: 8, flexWrap: 'wrap' }}>
                  {paquetes.map((p) => (
                    <button key={p.id} type="button" onClick={() => agregarPaquete(p)} disabled={ocupado}>
                      <Package size={13} /> {p.nombre} · S/ {p.precio.toFixed(2)}
                    </button>
                  ))}
                </div>
              )}
            </>
          )}
        </div>
        <div className="pm-modal-pie">
          {editable && (
            <button className="pm-boton-peligro" onClick={anular} disabled={ocupado}>
              Anular
            </button>
          )}
          <button className="pm-boton-secundario" onClick={() => window.print()}>
            <Printer size={15} /> Imprimir
          </button>
          {orden.estado === 'ABIERTA' && (
            <button className="pm-boton-secundario" onClick={() => ejecutar(() => api.ordenEstado(orden.id, 'TERMINADA'))} disabled={ocupado}>
              <Wrench size={15} /> Terminada
            </button>
          )}
          {orden.estado === 'TERMINADA' && (
            <button className="pm-boton-secundario" onClick={() => ejecutar(() => api.ordenEstado(orden.id, 'ABIERTA'))} disabled={ocupado}>
              Reabrir
            </button>
          )}
          <button className="pm-boton-secundario" onClick={onCerrar}>
            Cerrar
          </button>
          {editable && items.length > 0 && (
            <button className="pm-boton" onClick={cobrar} disabled={ocupado}>
              <ShoppingCart size={15} /> Cobrar
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** Comisión de cada mecánico en un periodo (solo administrador). */
function Comisiones() {
  const [desde, setDesde] = useState(() => `${hoyLima().slice(0, 8)}01`);
  const [hasta, setHasta] = useState(() => hoyLima());
  const [lista, setLista] = useState(null);
  const [error, setError] = useState(null);
  useEffect(() => {
    api
      .ordenesComisiones(desde, hasta)
      .then((d) => {
        setLista(d);
        setError(null);
      })
      .catch((e) => setError(e.message));
  }, [desde, hasta]);
  const total = (lista || []).reduce((s, c) => s + c.comision, 0);
  return (
    <>
      <div className="pm-campos" style={{ marginBottom: 12 }}>
        <label className="pm-campo">
          <span>Desde</span>
          <input type="date" value={desde} onChange={(e) => setDesde(e.target.value)} />
        </label>
        <label className="pm-campo">
          <span>Hasta</span>
          <input type="date" value={hasta} onChange={(e) => setHasta(e.target.value)} />
        </label>
      </div>
      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
      <div className="pm-tarjeta">
        {lista === null ? (
          <p className="pm-vacio">Cargando...</p>
        ) : lista.length === 0 ? (
          <p className="pm-vacio">No hay órdenes cobradas en esas fechas.</p>
        ) : (
          <table className="pm-lineas">
            <thead>
              <tr>
                <th>Mecánico</th>
                <th className="pm-num">Órdenes</th>
                <th className="pm-num">Mano de obra</th>
                <th className="pm-num">Repuestos</th>
                <th className="pm-num">Comisión</th>
              </tr>
            </thead>
            <tbody>
              {lista.map((c) => (
                <tr key={c.mecanico_id ?? 'ninguno'}>
                  <td>
                    {c.mecanico_nombre}
                    <small>{c.mecanico_id ? `${formatoCantidad(c.comision_porcentaje)} % de la mano de obra` : 'Órdenes sin mecánico'}</small>
                  </td>
                  <td className="pm-num">{c.ordenes}</td>
                  <td className="pm-num">S/ {c.mano_de_obra.toFixed(2)}</td>
                  <td className="pm-num">S/ {c.repuestos.toFixed(2)}</td>
                  <td className="pm-num">
                    <strong>S/ {c.comision.toFixed(2)}</strong>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {lista?.length > 0 && (
          <div className="pm-total">
            <span>Total de comisiones</span>
            <strong>S/ {total.toFixed(2)}</strong>
          </div>
        )}
      </div>
      <p className="pm-subtitulo" style={{ marginTop: 8 }}>
        Cuenta las órdenes cobradas cuya venta sigue vigente. La mano de obra son los productos que se venden sin controlar stock.
      </p>
    </>
  );
}

/** Alta y edición de mecánicos (solo administrador). */
function Mecanicos({ mecanicos, onCambio }) {
  const [form, setForm] = useState(null);
  const [error, setError] = useState(null);
  const guardar = async () => {
    const datos = {
      nombre: form.nombre,
      telefono: form.telefono || null,
      comision_porcentaje: Number(String(form.comision_porcentaje || '0').replace(',', '.')),
      activo: form.activo,
    };
    try {
      if (form.id) await api.mecanicoActualizar(form.id, datos);
      else await api.mecanicoCrear(datos);
      setForm(null);
      setError(null);
      onCambio();
    } catch (e) {
      setError(e.message);
    }
  };
  return (
    <>
      <button className="pm-boton" style={{ marginBottom: 12 }} onClick={() => setForm({ nombre: '', telefono: '', comision_porcentaje: '', activo: true })}>
        <Users size={15} /> Agregar mecánico
      </button>
      <div className="pm-tarjeta">
        {mecanicos.length === 0 ? (
          <p className="pm-vacio">Agrega a los mecánicos para asignarles órdenes y calcular su comisión.</p>
        ) : (
          mecanicos.map((m) => (
            <button key={m.id} className="pm-fila" onClick={() => setForm({ ...m, telefono: m.telefono || '', comision_porcentaje: String(m.comision_porcentaje) })}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">
                  {m.nombre} {!m.activo && <span className="pm-chip">De baja</span>}
                </span>
                <span className="pm-fila-detalle" style={{ display: 'block' }}>
                  {formatoCantidad(m.comision_porcentaje)} % de la mano de obra{m.telefono ? ` · ${m.telefono}` : ''}
                </span>
              </span>
            </button>
          ))
        )}
      </div>
      {form && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setForm(null)}>
          <div className="pm-modal pm-modal-chico" role="dialog" aria-modal="true" aria-label="Mecánico">
            <div className="pm-modal-cabecera">
              <h2>{form.id ? 'Editar mecánico' : 'Nuevo mecánico'}</h2>
            </div>
            <div className="pm-modal-cuerpo">
              {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
              <div className="pm-campos">
                <label className="pm-campo pm-campo-ancho">
                  <span>Nombre</span>
                  <input autoFocus value={form.nombre} maxLength={80} onChange={(e) => setForm((f) => ({ ...f, nombre: e.target.value }))} />
                </label>
                <label className="pm-campo">
                  <span>Celular</span>
                  <input value={form.telefono} maxLength={20} onChange={(e) => setForm((f) => ({ ...f, telefono: e.target.value }))} />
                </label>
                <label className="pm-campo">
                  <span>Comisión (%)</span>
                  <input
                    inputMode="decimal"
                    value={form.comision_porcentaje}
                    placeholder="0"
                    onChange={(e) => setForm((f) => ({ ...f, comision_porcentaje: e.target.value }))}
                  />
                </label>
                {form.id && (
                  <label className="pm-campo pm-campo-ancho" style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
                    <input type="checkbox" style={{ width: 'auto' }} checked={form.activo} onChange={(e) => setForm((f) => ({ ...f, activo: e.target.checked }))} />
                    Sigue trabajando aquí
                  </label>
                )}
              </div>
            </div>
            <div className="pm-modal-pie">
              <button className="pm-boton-secundario" onClick={() => setForm(null)}>
                Cancelar
              </button>
              <button className="pm-boton" onClick={guardar}>
                Guardar
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
