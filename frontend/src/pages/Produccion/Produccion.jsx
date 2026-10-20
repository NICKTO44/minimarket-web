import { Fragment, useCallback, useEffect, useMemo, useState } from 'react';
import { BookOpen, ChevronDown, ChevronUp, PackageMinus, PackagePlus, Pencil, Plus, Trash2, Wheat } from 'lucide-react';
import { api } from '../../api/api';
import { fechaCorta, hoyLima } from '../../utils/formato';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import { confirmar } from '../../utils/confirmar';
import '../../components/PantallaModulo.css';
import './Produccion.css';

const MOTIVOS = [
  { valor: 'NO_VENDIDO', label: 'No se vendió' },
  { valor: 'MALOGRADO', label: 'Se malogró' },
  { valor: 'VENCIDO', label: 'Venció' },
  { valor: 'OTRO', label: 'Otro' },
];
const NOMBRE_MOTIVO = Object.fromEntries(MOTIVOS.map((m) => [m.valor, m.label]));

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;
const numeroDe = (texto) => Number(String(texto ?? '').replace(',', '.'));
const hora = (fechaHora) => String(fechaHora || '').slice(11, 16);
const unidad = (u) => abreviaturaUnidad(u || 'UNIDAD');

/** Buscador de productos: escribe y elige. */
function BuscarProducto({ productos, placeholder, onElegir, filtro }) {
  const [q, setQ] = useState('');
  const encontrados = useMemo(() => {
    const t = q.trim().toLowerCase();
    if (t.length < 2) return [];
    return productos
      .filter((p) => (!filtro || filtro(p)) && (p.nombre.toLowerCase().includes(t) || String(p.codigo || '').toLowerCase().includes(t)))
      .slice(0, 8);
  }, [productos, q, filtro]);
  return (
    <div className="prd-buscar">
      <input value={q} placeholder={placeholder} onChange={(e) => setQ(e.target.value)} />
      {encontrados.length > 0 && (
        <div className="prd-buscar-lista">
          {encontrados.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => {
                setQ('');
                onElegir(p);
              }}
            >
              <span>{p.nombre}</span>
              <small>
                stock {formatoCantidad(p.stock)} {unidad(p.unidad_medida)}
              </small>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Producción y mermas (módulo PRODUCCION; recetas con el módulo INSUMOS).
 * Lo producido entra al stock sin ser una compra; lo perdido sale con su
 * costo. Con receta, al producir se descuentan los insumos.
 */
export default function Produccion({ esAdmin = false, conRecetas = false }) {
  const [vista, setVista] = useState('DIA');
  const vistas = [
    ['DIA', 'Hoy'],
    ['HISTORIAL', 'Historial'],
    ...(conRecetas && esAdmin ? [['RECETAS', 'Recetas y costos']] : []),
  ];
  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <Wheat size={22} /> Producción y mermas
          </h1>
          <p className="pm-subtitulo">
            Lo que produces entra al stock sin hacerlo como compra. Lo que se pierde sale del stock con su costo.
          </p>
        </div>
        <div className="pm-acciones">
          <div className="pm-filtros" role="tablist">
            {vistas.map(([valor, label]) => (
              <button key={valor} role="tab" aria-selected={vista === valor} className={vista === valor ? 'activo' : ''} onClick={() => setVista(valor)}>
                {label}
              </button>
            ))}
          </div>
        </div>
      </header>
      {vista === 'DIA' && <Dia esAdmin={esAdmin} conRecetas={conRecetas} />}
      {vista === 'HISTORIAL' && <Historial esAdmin={esAdmin} />}
      {vista === 'RECETAS' && <Recetas />}
    </div>
  );
}

// ---------------------------------------------------------------------
// Hoy: registrar producción y mermas, y el resumen del día
// ---------------------------------------------------------------------

function Dia({ esAdmin, conRecetas }) {
  const hoy = hoyLima();
  const [productos, setProductos] = useState([]);
  const [formulas, setFormulas] = useState([]);
  const [datos, setDatos] = useState({ producciones: [], mermas: [], resumen: null });
  const [error, setError] = useState(null);
  const [aviso, setAviso] = useState(null);

  const cargar = useCallback(() => {
    Promise.all([api.produccion(hoy, hoy), api.mermas(hoy, hoy), api.produccionResumen(hoy, hoy)])
      .then(([producciones, mermas, resumen]) => {
        setDatos({ producciones, mermas, resumen });
        setError(null);
      })
      .catch((e) => setError(e.message));
    api.productos().then(setProductos).catch(() => {});
  }, [hoy]);

  useEffect(() => {
    cargar();
    if (conRecetas) api.formulas().then(setFormulas).catch(() => setFormulas([]));
  }, [cargar, conRecetas]);

  const listo = (texto) => {
    setAviso(texto);
    cargar();
  };

  return (
    <>
      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
      {aviso && <p className="pm-mensaje pm-mensaje-exito">{aviso}</p>}
      <div className="prd-formularios">
        <FormProduccion productos={productos} formulas={formulas} conRecetas={conRecetas} onListo={listo} onError={setError} />
        <FormMerma productos={productos} onListo={listo} onError={setError} />
      </div>
      <ResumenTabla resumen={datos.resumen} titulo="Resumen de hoy" esAdmin={esAdmin} />
      <Movimientos producciones={datos.producciones} mermas={datos.mermas} esAdmin={esAdmin} onCambio={cargar} onError={setError} />
    </>
  );
}

function FormProduccion({ productos, formulas, conRecetas, onListo, onError }) {
  const [producto, setProducto] = useState(null);
  const [cantidad, setCantidad] = useState('');
  const [vence, setVence] = useState('');
  const [nota, setNota] = useState('');
  const [usarReceta, setUsarReceta] = useState(true);
  const [faltantes, setFaltantes] = useState(null);
  const [ocupado, setOcupado] = useState(false);

  const formula = producto && conRecetas ? formulas.find((f) => f.producto_id === producto.id) : null;
  const cant = numeroDe(cantidad);
  const porInsumo = formula && cant > 0 ? formula.insumos.map((i) => ({ ...i, necesita: (i.cantidad * cant) / formula.rinde })) : [];

  const reiniciar = () => {
    setProducto(null);
    setCantidad('');
    setVence('');
    setNota('');
    setUsarReceta(true);
    setFaltantes(null);
  };

  const registrar = async (forzar = false) => {
    onError(null);
    if (!producto) return onError('Elige qué se produjo.');
    if (!(cant > 0)) return onError('Escribe cuánto se produjo.');
    if (producto.lleva_vencimiento && !vence) return onError(`${producto.nombre} lleva vencimiento: elige hasta cuándo dura.`);
    setOcupado(true);
    try {
      const p = await api.produccionRegistrar({
        producto_id: producto.id,
        cantidad: cant,
        fecha_vencimiento: producto.lleva_vencimiento ? vence : null,
        nota: nota.trim() || null,
        usar_receta: usarReceta,
        forzar,
      });
      reiniciar();
      onListo(`Producción registrada: ${formatoCantidad(p.cantidad)} ${unidad(p.unidad_medida)} de ${p.producto_nombre}.`);
    } catch (e) {
      if (e.message.startsWith('No alcanza el stock de:')) setFaltantes(e.message);
      else onError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <section className="pm-tarjeta prd-form">
      <h2>
        <PackagePlus size={18} /> Registrar producción
      </h2>
      {producto ? (
        <div className="prd-elegido">
          <strong>{producto.nombre}</strong>
          <small>
            Stock: {formatoCantidad(producto.stock)} {unidad(producto.unidad_medida)}
          </small>
          <button type="button" className="pm-boton-secundario" onClick={reiniciar}>
            Cambiar
          </button>
        </div>
      ) : (
        <BuscarProducto
          productos={productos}
          placeholder="¿Qué se produjo? Pan, torta, alfajor…"
          filtro={(p) => p.controla_stock !== false}
          onElegir={(p) => {
            setProducto(p);
            setFaltantes(null);
          }}
        />
      )}
      {producto && (
        <>
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Cantidad ({unidad(producto.unidad_medida)})</span>
              <input autoFocus inputMode="decimal" value={cantidad} onChange={(e) => setCantidad(e.target.value)} />
            </label>
            {producto.lleva_vencimiento && (
              <label className="pm-campo">
                <span>Vence el</span>
                <input type="date" min={hoyLima()} value={vence} onChange={(e) => setVence(e.target.value)} />
              </label>
            )}
            <label className="pm-campo pm-campo-ancho">
              <span>Nota (opcional)</span>
              <input value={nota} maxLength={300} placeholder="Turno mañana" onChange={(e) => setNota(e.target.value)} />
            </label>
          </div>
          {formula && (
            <div className="prd-receta">
              <label className="prd-check">
                <input type="checkbox" checked={usarReceta} onChange={(e) => setUsarReceta(e.target.checked)} />
                Descontar insumos de la receta (rinde {formatoCantidad(formula.rinde)} {unidad(producto.unidad_medida)})
              </label>
              {usarReceta && porInsumo.length > 0 && (
                <ul>
                  {porInsumo.map((i) => (
                    <li key={i.insumo_id} className={i.stock + 1e-6 < i.necesita ? 'prd-falta' : ''}>
                      {i.nombre}: {formatoCantidad(Math.round(i.necesita * 1000) / 1000)} {unidad(i.unidad_medida)}
                      <small> (hay {formatoCantidad(i.stock)})</small>
                    </li>
                  ))}
                </ul>
              )}
              {usarReceta && formula.costo_unitario != null && cant > 0 && (
                <p className="pm-subtitulo">
                  Costo estimado: {soles(formula.costo_unitario * cant)} ({soles(formula.costo_unitario)} c/u)
                </p>
              )}
            </div>
          )}
          {conRecetas && !formula && <p className="pm-subtitulo">Este producto no tiene receta: solo sube su stock.</p>}
          {faltantes && (
            <div className="prd-faltan">
              <p>{faltantes}</p>
              <button type="button" className="pm-boton-secundario" onClick={() => registrar(true)} disabled={ocupado}>
                Registrar igual (descuenta lo que haya)
              </button>
            </div>
          )}
          <button className="pm-boton prd-enviar" onClick={() => registrar(false)} disabled={ocupado}>
            {ocupado ? 'Guardando...' : 'Registrar producción'}
          </button>
        </>
      )}
    </section>
  );
}

function FormMerma({ productos, onListo, onError }) {
  const [producto, setProducto] = useState(null);
  const [cantidad, setCantidad] = useState('');
  const [motivo, setMotivo] = useState('NO_VENDIDO');
  const [nota, setNota] = useState('');
  const [ocupado, setOcupado] = useState(false);

  const registrar = async () => {
    onError(null);
    const cant = numeroDe(cantidad);
    if (!producto) return onError('Elige qué se perdió.');
    if (!(cant > 0)) return onError('Escribe cuánto se perdió.');
    setOcupado(true);
    try {
      const m = await api.mermaRegistrar({ producto_id: producto.id, cantidad: cant, motivo, nota: nota.trim() || null });
      setProducto(null);
      setCantidad('');
      setNota('');
      onListo(`Merma registrada: ${formatoCantidad(m.cantidad)} ${unidad(m.unidad_medida)} de ${m.producto_nombre}.`);
    } catch (e) {
      onError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <section className="pm-tarjeta prd-form">
      <h2>
        <PackageMinus size={18} /> Registrar merma
      </h2>
      {producto ? (
        <div className="prd-elegido">
          <strong>{producto.nombre}</strong>
          <small>
            Stock: {formatoCantidad(producto.stock)} {unidad(producto.unidad_medida)}
          </small>
          <button type="button" className="pm-boton-secundario" onClick={() => setProducto(null)}>
            Cambiar
          </button>
        </div>
      ) : (
        <BuscarProducto
          productos={productos}
          placeholder="¿Qué se perdió?"
          filtro={(p) => p.controla_stock !== false && p.stock > 0}
          onElegir={setProducto}
        />
      )}
      {producto && (
        <>
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Cantidad ({unidad(producto.unidad_medida)})</span>
              <input autoFocus inputMode="decimal" value={cantidad} onChange={(e) => setCantidad(e.target.value)} />
            </label>
            <div className="pm-campo pm-campo-ancho">
              <div className="pm-opciones">
                {MOTIVOS.map((m) => (
                  <button key={m.valor} type="button" className={motivo === m.valor ? 'activo' : ''} onClick={() => setMotivo(m.valor)}>
                    {m.label}
                  </button>
                ))}
              </div>
            </div>
            <label className="pm-campo pm-campo-ancho">
              <span>Nota (opcional)</span>
              <input value={nota} maxLength={300} onChange={(e) => setNota(e.target.value)} />
            </label>
          </div>
          <button className="pm-boton prd-enviar" onClick={registrar} disabled={ocupado}>
            {ocupado ? 'Guardando...' : 'Registrar merma'}
          </button>
        </>
      )}
    </section>
  );
}

function ResumenTabla({ resumen, titulo, esAdmin }) {
  if (!resumen) return null;
  return (
    <section className="pm-tarjeta prd-seccion">
      <h2>{titulo}</h2>
      {resumen.filas.length === 0 ? (
        <p className="pm-vacio">Todavía no hay producción ni mermas registradas.</p>
      ) : (
        <div className="prd-tabla-scroll">
          <table className="pm-lineas">
            <thead>
              <tr>
                <th>Producto</th>
                <th className="pm-num">Producido</th>
                <th className="pm-num">Vendido</th>
                <th className="pm-num">Merma</th>
                <th className="pm-num">Stock hoy</th>
                {esAdmin && <th className="pm-num">Costo producción</th>}
                {esAdmin && <th className="pm-num">Costo merma</th>}
              </tr>
            </thead>
            <tbody>
              {resumen.filas.map((f) => (
                <tr key={f.producto_id}>
                  <td>{f.producto_nombre}</td>
                  <td className="pm-num">
                    {formatoCantidad(f.producido)} {unidad(f.unidad_medida)}
                  </td>
                  <td className="pm-num">{formatoCantidad(f.vendido)}</td>
                  <td className={`pm-num${f.merma > 0 ? ' prd-rojo' : ''}`}>{formatoCantidad(f.merma)}</td>
                  <td className="pm-num">{formatoCantidad(f.stock)}</td>
                  {esAdmin && <td className="pm-num">{f.costo_produccion != null ? soles(f.costo_produccion) : f.producido > 0 ? '—' : ''}</td>}
                  {esAdmin && <td className="pm-num">{f.costo_merma != null ? soles(f.costo_merma) : f.merma > 0 ? '—' : ''}</td>}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {esAdmin && resumen.filas.length > 0 && (
        <div className="prd-totales">
          <span>
            Costo de producción: <strong>{soles(resumen.costo_produccion)}</strong>
          </span>
          <span>
            Pérdida por mermas: <strong className="prd-rojo">{soles(resumen.costo_merma)}</strong>
          </span>
          {resumen.producciones_sin_costo > 0 && (
            <small>“—”: sin receta o con insumos sin precio de compra, no se sabe su costo.</small>
          )}
        </div>
      )}
    </section>
  );
}

function Movimientos({ producciones, mermas, esAdmin, onCambio, onError }) {
  const anular = async (tipo, item) => {
    const esProduccion = tipo === 'produccion';
    const ok = await confirmar({
      titulo: esProduccion ? `¿Anular la producción de ${item.producto_nombre}?` : `¿Anular la merma de ${item.producto_nombre}?`,
      mensaje: esProduccion
        ? 'Lo producido sale del stock y los insumos vuelven. Si ya se vendió, no se puede.'
        : 'Lo descontado vuelve al stock.',
      textoConfirmar: 'Anular',
    });
    if (!ok) return;
    try {
      await (esProduccion ? api.produccionAnular(item.id) : api.mermaAnular(item.id));
      onCambio();
    } catch (e) {
      onError(e.message);
    }
  };
  return (
    <div className="prd-movimientos">
      <section className="pm-tarjeta prd-seccion">
        <h2>Producción</h2>
        {producciones.length === 0 ? (
          <p className="pm-vacio">Sin producción registrada.</p>
        ) : (
          producciones.map((p) => (
            <div key={p.id} className={`prd-mov${p.estado === 'ANULADA' ? ' prd-anulada' : ''}`}>
              <div>
                <strong>
                  {formatoCantidad(p.cantidad)} {unidad(p.unidad_medida)} · {p.producto_nombre}
                </strong>
                <small>
                  {fechaCorta(p.fecha_hora)} {hora(p.fecha_hora)} · {p.usuario_nombre || ''}
                  {p.fecha_vencimiento ? ` · vence ${fechaCorta(p.fecha_vencimiento)}` : ''}
                  {p.nota ? ` · ${p.nota}` : ''}
                  {p.estado === 'ANULADA' ? ` · anulada${p.anulada_por ? ` por ${p.anulada_por}` : ''}` : ''}
                </small>
                {p.insumos.length > 0 && (
                  <small className="prd-insumos">
                    Insumos:{' '}
                    {p.insumos
                      .map((i) => `${formatoCantidad(i.cantidad)} ${unidad(i.unidad_medida)} ${i.nombre}${i.faltante > 0 ? ` (faltó ${formatoCantidad(i.faltante)})` : ''}`)
                      .join(', ')}
                  </small>
                )}
              </div>
              <div className="prd-mov-lado">
                {esAdmin && p.costo_total != null && <span>{soles(p.costo_total)}</span>}
                {esAdmin && p.estado === 'REGISTRADA' && (
                  <button className="pm-boton-secundario" onClick={() => anular('produccion', p)}>
                    Anular
                  </button>
                )}
              </div>
            </div>
          ))
        )}
      </section>
      <section className="pm-tarjeta prd-seccion">
        <h2>Mermas</h2>
        {mermas.length === 0 ? (
          <p className="pm-vacio">Sin mermas registradas.</p>
        ) : (
          mermas.map((m) => (
            <div key={m.id} className={`prd-mov${m.estado === 'ANULADA' ? ' prd-anulada' : ''}`}>
              <div>
                <strong>
                  {formatoCantidad(m.cantidad)} {unidad(m.unidad_medida)} · {m.producto_nombre}
                </strong>
                <small>
                  {NOMBRE_MOTIVO[m.motivo] || m.motivo} · {fechaCorta(m.fecha_hora)} {hora(m.fecha_hora)} · {m.usuario_nombre || ''}
                  {m.nota ? ` · ${m.nota}` : ''}
                  {m.estado === 'ANULADA' ? ` · anulada${m.anulada_por ? ` por ${m.anulada_por}` : ''}` : ''}
                </small>
              </div>
              <div className="prd-mov-lado">
                {esAdmin && m.costo_total != null && <span className="prd-rojo">− {soles(m.costo_total)}</span>}
                {esAdmin && m.estado === 'REGISTRADA' && (
                  <button className="pm-boton-secundario" onClick={() => anular('merma', m)}>
                    Anular
                  </button>
                )}
              </div>
            </div>
          ))
        )}
      </section>
    </div>
  );
}

// ---------------------------------------------------------------------
// Historial por fechas
// ---------------------------------------------------------------------

function Historial({ esAdmin }) {
  const [desde, setDesde] = useState(() => hoyLima(-6));
  const [hasta, setHasta] = useState(() => hoyLima());
  const [datos, setDatos] = useState({ producciones: [], mermas: [], resumen: null });
  const [error, setError] = useState(null);
  const cargar = useCallback(() => {
    Promise.all([api.produccion(desde, hasta), api.mermas(desde, hasta), api.produccionResumen(desde, hasta)])
      .then(([producciones, mermas, resumen]) => {
        setDatos({ producciones, mermas, resumen });
        setError(null);
      })
      .catch((e) => setError(e.message));
  }, [desde, hasta]);
  useEffect(() => {
    cargar();
  }, [cargar]);
  return (
    <>
      <div className="pm-campos" style={{ marginBottom: 12 }}>
        <label className="pm-campo">
          <span>Desde</span>
          <input type="date" value={desde} max={hasta} onChange={(e) => setDesde(e.target.value)} />
        </label>
        <label className="pm-campo">
          <span>Hasta</span>
          <input type="date" value={hasta} min={desde} onChange={(e) => setHasta(e.target.value)} />
        </label>
      </div>
      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
      <ResumenTabla resumen={datos.resumen} titulo={`Del ${fechaCorta(desde)} al ${fechaCorta(hasta)}`} esAdmin={esAdmin} />
      <Movimientos producciones={datos.producciones} mermas={datos.mermas} esAdmin={esAdmin} onCambio={cargar} onError={setError} />
    </>
  );
}

// ---------------------------------------------------------------------
// Recetas de producción y costos (solo administrador)
// ---------------------------------------------------------------------

function Recetas() {
  const [formulas, setFormulas] = useState(null);
  const [productos, setProductos] = useState([]);
  const [editando, setEditando] = useState(null);
  // Receta desplegada en la tabla (producto_id) para verla completa.
  const [abierta, setAbierta] = useState(null);
  const [error, setError] = useState(null);
  const cargar = useCallback(() => {
    api
      .formulas()
      .then((f) => {
        setFormulas(f);
        setError(null);
      })
      .catch((e) => setError(e.message));
  }, []);
  useEffect(() => {
    cargar();
    api.productos().then(setProductos).catch(() => {});
  }, [cargar]);

  return (
    <>
      <div className="prd-barra">
        <p className="pm-subtitulo">
          Cada receta dice cuánto rinde y qué insumos lleva. Los insumos son productos (harina, huevos…) con su precio de compra: así se calcula cuánto
          cuesta hacer cada producto.
        </p>
        <button className="pm-boton" onClick={() => setEditando({})}>
          <Plus size={15} /> Nueva receta
        </button>
      </div>
      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
      <div className="pm-tarjeta">
        {formulas === null ? (
          <p className="pm-vacio">Cargando...</p>
        ) : formulas.length === 0 ? (
          <p className="pm-vacio">Todavía no hay recetas. Crea la del producto que más produces.</p>
        ) : (
          <div className="prd-tabla-scroll">
            <table className="pm-lineas">
              <thead>
                <tr>
                  <th>Producto</th>
                  <th className="pm-num">Rinde</th>
                  <th className="pm-num">Costo c/u</th>
                  <th className="pm-num">Precio</th>
                  <th className="pm-num">Ganancia c/u</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {formulas.map((f) => (
                  <Fragment key={f.producto_id}>
                    <tr className="prd-fila-click" onClick={() => setAbierta((a) => (a === f.producto_id ? null : f.producto_id))}>
                      <td>
                        {f.producto_nombre}
                        <small>{f.insumos.map((i) => i.nombre).join(', ')}</small>
                        {f.sin_costo.length > 0 && <small className="prd-rojo">Sin precio de compra: {f.sin_costo.join(', ')}</small>}
                      </td>
                      <td className="pm-num">
                        {formatoCantidad(f.rinde)} {unidad(f.unidad_medida)}
                      </td>
                      <td className="pm-num">{f.costo_unitario != null ? soles(f.costo_unitario) : '—'}</td>
                      <td className="pm-num">{soles(f.precio)}</td>
                      <td className={`pm-num${f.margen != null && f.margen < 0 ? ' prd-rojo' : ''}`}>
                        {f.margen != null ? (
                          <>
                            <strong>{soles(f.margen)}</strong>
                            <small>{f.margen_porcentaje != null ? `${formatoCantidad(f.margen_porcentaje)} %` : ''}</small>
                          </>
                        ) : (
                          '—'
                        )}
                      </td>
                      <td className="prd-receta-acciones">
                        <button
                          type="button"
                          className="pm-boton-secundario"
                          onClick={(e) => {
                            e.stopPropagation();
                            setAbierta((a) => (a === f.producto_id ? null : f.producto_id));
                          }}
                          aria-expanded={abierta === f.producto_id}
                        >
                          {abierta === f.producto_id ? <ChevronUp size={14} /> : <ChevronDown size={14} />} {abierta === f.producto_id ? 'Ocultar' : 'Ver receta'}
                        </button>
                        <button
                          type="button"
                          className="pm-boton-secundario"
                          onClick={(e) => {
                            e.stopPropagation();
                            setEditando({ formula: f });
                          }}
                          aria-label={`Editar receta de ${f.producto_nombre}`}
                        >
                          <Pencil size={14} /> Editar
                        </button>
                      </td>
                    </tr>
                    {abierta === f.producto_id && (
                      <tr className="prd-receta-detalle">
                        <td colSpan={6}>
                          <p className="prd-receta-titulo">
                            Para {formatoCantidad(f.rinde)} {unidad(f.unidad_medida)} de {f.producto_nombre}:
                          </p>
                          <table className="pm-lineas">
                            <thead>
                              <tr>
                                <th>Insumo</th>
                                <th className="pm-num">Cantidad</th>
                                <th className="pm-num">Costo unit.</th>
                                <th className="pm-num">Costo</th>
                                <th className="pm-num">Stock</th>
                              </tr>
                            </thead>
                            <tbody>
                              {f.insumos.map((i) => (
                                <tr key={i.insumo_id}>
                                  <td>{i.nombre}</td>
                                  <td className="pm-num">
                                    {formatoCantidad(i.cantidad)} {unidad(i.unidad_medida)}
                                  </td>
                                  <td className="pm-num">{i.costo_unitario != null ? soles(i.costo_unitario) : '—'}</td>
                                  <td className="pm-num">{i.costo != null ? soles(i.costo) : '—'}</td>
                                  <td className={`pm-num${i.stock < i.cantidad ? ' prd-rojo' : ''}`}>
                                    {formatoCantidad(i.stock)} {unidad(i.unidad_medida)}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                            {f.costo_total != null && (
                              <tfoot>
                                <tr>
                                  <td colSpan={3}>
                                    <strong>Costo de la receta</strong>
                                  </td>
                                  <td className="pm-num">
                                    <strong>{soles(f.costo_total)}</strong>
                                  </td>
                                  <td />
                                </tr>
                              </tfoot>
                            )}
                          </table>
                          {f.notas && (
                            <p className="prd-receta-notas">
                              <strong>Preparación:</strong> {f.notas}
                            </p>
                          )}
                        </td>
                      </tr>
                    )}
                  </Fragment>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
      {editando && (
        <EditorReceta
          formula={editando.formula || null}
          productos={productos}
          existentes={(formulas || []).map((f) => f.producto_id)}
          onCerrar={() => setEditando(null)}
          onGuardado={() => {
            setEditando(null);
            cargar();
          }}
        />
      )}
    </>
  );
}

/** Costo de una unidad del insumo según la lista de productos (aproximado; el servidor da el exacto). */
const costoDe = (p) => (p?.costo_promedio > 0 ? p.costo_promedio : p?.precio_compra > 0 ? p.precio_compra : null);

function EditorReceta({ formula, productos, existentes, onCerrar, onGuardado }) {
  const porId = useMemo(() => new Map(productos.map((p) => [p.id, p])), [productos]);
  const [producto, setProducto] = useState(() => (formula ? { id: formula.producto_id, nombre: formula.producto_nombre, unidad_medida: formula.unidad_medida, precio: formula.precio } : null));
  const [rinde, setRinde] = useState(formula ? String(formula.rinde) : '');
  const [notas, setNotas] = useState(formula?.notas || '');
  const [lineas, setLineas] = useState(() =>
    (formula?.insumos || []).map((i) => ({ insumo_id: i.insumo_id, nombre: i.nombre, unidad_medida: i.unidad_medida, cantidad: String(i.cantidad), costo: i.costo_unitario }))
  );
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);

  const costoTotal = lineas.reduce((s, l) => s + (l.costo != null ? l.costo * (numeroDe(l.cantidad) || 0) : 0), 0);
  const faltaCosto = lineas.some((l) => l.costo == null);
  const r = numeroDe(rinde);
  const costoUnidad = r > 0 && !faltaCosto && lineas.length > 0 ? costoTotal / r : null;

  const agregar = (p) => {
    if (lineas.some((l) => l.insumo_id === p.id)) return;
    setLineas((ls) => [...ls, { insumo_id: p.id, nombre: p.nombre, unidad_medida: p.unidad_medida, cantidad: '', costo: costoDe(p) }]);
  };

  const guardar = async () => {
    setError(null);
    if (!producto) return setError('Elige el producto.');
    if (!(r > 0)) return setError('Escribe cuánto rinde la receta.');
    if (lineas.length === 0) return setError('Agrega los insumos.');
    if (lineas.some((l) => !(numeroDe(l.cantidad) > 0))) return setError('Escribe la cantidad de cada insumo.');
    setOcupado(true);
    try {
      await api.formulaGuardar(producto.id, {
        rinde: r,
        notas: notas.trim() || null,
        insumos: lineas.map((l) => ({ insumo_id: l.insumo_id, cantidad: numeroDe(l.cantidad) })),
      });
      onGuardado();
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  const borrar = async () => {
    if (!(await confirmar({ titulo: `¿Borrar la receta de ${producto.nombre}?`, mensaje: 'El producto sigue existiendo; solo se borra su receta.', textoConfirmar: 'Borrar' })))
      return;
    try {
      await api.formulaBorrar(producto.id);
      onGuardado();
    } catch (e) {
      setError(e.message);
    }
  };

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Receta de producción">
        <div className="pm-modal-cabecera">
          <h2>
            <BookOpen size={18} /> {formula ? `Receta de ${formula.producto_nombre}` : 'Nueva receta de producción'}
          </h2>
          <p>Las cantidades van en la unidad de cada insumo (kg, litro, unidad…).</p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          {!formula &&
            (producto ? (
              <div className="prd-elegido">
                <strong>{producto.nombre}</strong>
                <button type="button" className="pm-boton-secundario" onClick={() => setProducto(null)}>
                  Cambiar
                </button>
              </div>
            ) : (
              <BuscarProducto
                productos={productos}
                placeholder="¿De qué producto es la receta?"
                filtro={(p) => !existentes.includes(p.id)}
                onElegir={setProducto}
              />
            ))}
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Rinde ({unidad(producto?.unidad_medida)})</span>
              <input inputMode="decimal" value={rinde} placeholder="50" onChange={(e) => setRinde(e.target.value)} />
            </label>
          </div>

          <p className="pm-seccion">Insumos</p>
          {lineas.length === 0 && <p className="pm-vacio">Busca abajo la harina, los huevos, la mantequilla…</p>}
          {lineas.map((l, idx) => (
            <div key={l.insumo_id} className="prd-linea">
              <span className="prd-linea-nombre">
                {l.nombre}
                <small>{l.costo != null ? `${soles(l.costo)} por ${unidad(l.unidad_medida)}` : 'sin precio de compra'}</small>
              </span>
              <input
                inputMode="decimal"
                value={l.cantidad}
                aria-label={`Cantidad de ${l.nombre}`}
                onChange={(e) => setLineas((ls) => ls.map((x, n) => (n === idx ? { ...x, cantidad: e.target.value } : x)))}
              />
              <span className="prd-linea-unidad">{unidad(l.unidad_medida)}</span>
              <span className="prd-linea-costo">{l.costo != null && numeroDe(l.cantidad) > 0 ? soles(l.costo * numeroDe(l.cantidad)) : ''}</span>
              <button
                type="button"
                className="pm-boton-secundario"
                aria-label={`Quitar ${l.nombre}`}
                onClick={() => setLineas((ls) => ls.filter((_, n) => n !== idx))}
              >
                <Trash2 size={14} />
              </button>
            </div>
          ))}
          <BuscarProducto
            productos={productos}
            placeholder="Agregar insumo: busca por nombre o código"
            filtro={(p) => p.id !== producto?.id && !lineas.some((l) => l.insumo_id === p.id) && porId.has(p.id)}
            onElegir={agregar}
          />

          <div className="prd-costos">
            <div>
              <span>Costo de la receta</span>
              <strong>{faltaCosto ? '—' : soles(costoTotal)}</strong>
            </div>
            <div>
              <span>Costo por {unidad(producto?.unidad_medida)}</span>
              <strong>{costoUnidad != null ? soles(costoUnidad) : '—'}</strong>
            </div>
            {producto && costoUnidad != null && (
              <div>
                <span>Ganancia c/u (precio {soles(producto.precio)})</span>
                <strong className={producto.precio - costoUnidad < 0 ? 'prd-rojo' : 'prd-verde'}>{soles(producto.precio - costoUnidad)}</strong>
              </div>
            )}
          </div>
          {faltaCosto && <p className="pm-subtitulo">Ponle precio de compra a los insumos en Productos para calcular el costo.</p>}

          <label className="pm-campo pm-campo-ancho" style={{ marginTop: 10 }}>
            <span>Preparación / notas (opcional)</span>
            <textarea rows={2} maxLength={1000} value={notas} onChange={(e) => setNotas(e.target.value)} />
          </label>
        </div>
        <div className="pm-modal-pie">
          {formula && (
            <button className="pm-boton-peligro" onClick={borrar} disabled={ocupado}>
              Borrar receta
            </button>
          )}
          <button className="pm-boton-secundario" onClick={onCerrar} disabled={ocupado}>
            Cancelar
          </button>
          <button className="pm-boton" onClick={guardar} disabled={ocupado}>
            {ocupado ? 'Guardando...' : 'Guardar receta'}
          </button>
        </div>
      </div>
    </div>
  );
}
