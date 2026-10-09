import { useCallback, useEffect, useMemo, useState } from 'react';
import { Ban, ChevronLeft, ChevronRight, Plus, Tags, X } from 'lucide-react';
import { api } from '../../api/api';
import { hoyLima } from '../../utils/formato';
import { METODOS_GASTO, etiquetaComprobante, etiquetaMetodo, soles } from '../../utils/gastos';
import FormularioGasto from '../../components/FormularioGasto';
import '../../components/FormularioGasto.css';
import './Gastos.css';

const MESES = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre'];
const nombreMes = (clave) => `${MESES[Number(clave.slice(5, 7)) - 1]} ${clave.slice(0, 4)}`;

/** "2026-10" ± n meses. */
function moverMes(clave, n) {
  const total = Number(clave.slice(0, 4)) * 12 + Number(clave.slice(5, 7)) - 1 + n;
  return `${Math.floor(total / 12)}-${String((total % 12) + 1).padStart(2, '0')}`;
}

/** Primer y último día de un mes "2026-10". */
function rangoMes(clave) {
  const [a, m] = clave.split('-').map(Number);
  const ultimo = new Date(Date.UTC(a, m, 0)).getUTCDate();
  return { desde: `${clave}-01`, hasta: `${clave}-${String(ultimo).padStart(2, '0')}` };
}

/** "2026-10-09" → "9 oct". */
const diaCorto = (fecha) => `${Number(fecha.slice(8, 10))} ${MESES[Number(fecha.slice(5, 7)) - 1].slice(0, 3)}`;

/**
 * Gastos del negocio: lo que se paga y no es mercadería (alquiler, luz,
 * sueldos…). Solo el administrador. Los gastos con efectivo de caja se
 * descuentan del cuadre; todos entran en la ganancia neta (Reportes).
 */
export default function Gastos() {
  const mesActual = hoyLima().slice(0, 7);
  const [mes, setMes] = useState(mesActual);
  const [filtro, setFiltro] = useState({ categoria_id: '', metodo_pago: '', anulados: false });
  const [datos, setDatos] = useState(null);
  const [anterior, setAnterior] = useState(null);
  const [categorias, setCategorias] = useState([]);
  const [error, setError] = useState('');
  const [aviso, setAviso] = useState('');
  const [ventana, setVentana] = useState(null); // 'NUEVO' | 'CATEGORIAS' | { anular: gasto }
  const [version, setVersion] = useState(0);

  const recargar = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => {
    let vivo = true;
    const rango = rangoMes(mes);
    api
      .gastos({ ...rango, ...filtro })
      .then((d) => {
        if (!vivo) return;
        setDatos(d);
        setError('');
      })
      .catch((e) => vivo && setError(e.message));
    // Total del mes anterior, para comparar.
    api
      .gastos(rangoMes(moverMes(mes, -1)))
      .then((d) => vivo && setAnterior(d.total))
      .catch(() => vivo && setAnterior(null));
    return () => {
      vivo = false;
    };
  }, [mes, filtro, version]);

  useEffect(() => {
    let vivo = true;
    api
      .gastoCategorias()
      .then((l) => vivo && setCategorias(l))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, [version]);

  const maxCategoria = useMemo(() => Math.max(1, ...(datos?.por_categoria || []).map((c) => c.total)), [datos]);
  const filtrado = filtro.categoria_id || filtro.metodo_pago;
  const diferencia = datos && anterior != null && !filtrado ? datos.total - anterior : null;

  const poner = (campo, valor) => setFiltro((f) => ({ ...f, [campo]: valor }));

  const guardado = (texto) => {
    setVentana(null);
    setAviso(texto);
    recargar();
  };

  return (
    <div className="gst-layout">
      <div className="gst-cabecera">
        <div>
          <h1>Gastos</h1>
          <p>Lo que paga el negocio y no es mercadería: alquiler, servicios, sueldos, movilidad…</p>
        </div>
        <div className="gst-cabecera-acciones">
          <button type="button" className="gst-boton" onClick={() => setVentana('CATEGORIAS')}>
            <Tags size={16} /> Categorías
          </button>
          <button type="button" className="gst-boton gst-boton-principal" onClick={() => setVentana('NUEVO')}>
            <Plus size={16} /> Nuevo gasto
          </button>
        </div>
      </div>

      <div className="gst-mes">
        <button type="button" onClick={() => setMes(moverMes(mes, -1))} aria-label="Mes anterior">
          <ChevronLeft size={18} />
        </button>
        <strong>{nombreMes(mes)}</strong>
        <button type="button" onClick={() => setMes(moverMes(mes, 1))} disabled={mes >= mesActual} aria-label="Mes siguiente">
          <ChevronRight size={18} />
        </button>
        {mes !== mesActual && (
          <button type="button" className="gst-mes-hoy" onClick={() => setMes(mesActual)}>
            Este mes
          </button>
        )}
      </div>

      {aviso && (
        <p className="gst-aviso" onClick={() => setAviso('')}>
          {aviso}
        </p>
      )}
      {error && <p className="gst-error">{error}</p>}
      {!datos && !error && <p className="gst-tenue">Cargando gastos…</p>}

      {datos && (
        <>
          <div className="gst-resumen">
            <div className="gst-tarjeta gst-tarjeta-total">
              <span>{filtrado ? 'Total filtrado' : 'Gastado en el mes'}</span>
              <strong>{soles(datos.total)}</strong>
              <small>
                {datos.cantidad} {datos.cantidad === 1 ? 'gasto' : 'gastos'}
                {diferencia != null &&
                  Math.abs(diferencia) >= 0.01 &&
                  ` · ${soles(Math.abs(diferencia))} ${diferencia > 0 ? 'más' : 'menos'} que el mes anterior`}
              </small>
            </div>

            <div className="gst-tarjeta gst-categorias">
              <span>Por categoría</span>
              {datos.por_categoria.length === 0 && <small>Sin gastos</small>}
              {datos.por_categoria.map((c) => (
                <div key={c.clave} className="gst-barra">
                  <div className="gst-barra-texto">
                    <span>{c.clave}</span>
                    <strong>{soles(c.total)}</strong>
                  </div>
                  <div className="gst-barra-fondo">
                    <div style={{ width: `${(c.total / maxCategoria) * 100}%` }} />
                  </div>
                </div>
              ))}
            </div>

            <div className="gst-tarjeta">
              <span>Cómo se pagó</span>
              {datos.por_metodo.length === 0 && <small>Sin gastos</small>}
              <ul className="gst-metodos">
                {datos.por_metodo.map((m) => (
                  <li key={m.clave}>
                    <span>{etiquetaMetodo(m.clave)}</span>
                    <strong>{soles(m.total)}</strong>
                  </li>
                ))}
              </ul>
            </div>
          </div>

          <div className="gst-filtros">
            <select value={filtro.categoria_id} onChange={(e) => poner('categoria_id', e.target.value)} aria-label="Categoría">
              <option value="">Todas las categorías</option>
              {categorias.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.nombre}
                  {c.activo ? '' : ' (desactivada)'}
                </option>
              ))}
            </select>
            <select value={filtro.metodo_pago} onChange={(e) => poner('metodo_pago', e.target.value)} aria-label="Forma de pago">
              <option value="">Todas las formas de pago</option>
              {METODOS_GASTO.map((m) => (
                <option key={m.valor} value={m.valor}>
                  {m.etiqueta}
                </option>
              ))}
            </select>
            <label className="gst-check">
              <input type="checkbox" checked={filtro.anulados} onChange={(e) => poner('anulados', e.target.checked)} />
              Ver anulados
            </label>
          </div>

          <div className="gst-tabla">
            {datos.gastos.length === 0 && (
              <p className="gst-vacio">
                No hay gastos {filtrado ? 'con ese filtro' : 'este mes'}.{' '}
                {!filtrado && (
                  <button type="button" className="gst-enlace" onClick={() => setVentana('NUEVO')}>
                    Registrar el primero
                  </button>
                )}
              </p>
            )}
            {datos.gastos.map((g) => (
              <div key={g.id} className={`gst-fila${g.anulado ? ' gst-anulado' : ''}`}>
                <span className="gst-fecha">{diaCorto(g.fecha)}</span>
                <div className="gst-detalle">
                  <strong>{g.descripcion}</strong>
                  <small>
                    {g.categoria}
                    {g.pagado_a ? ` · ${g.pagado_a}` : ''}
                    {g.comprobante_tipo ? ` · ${etiquetaComprobante(g.comprobante_tipo)}${g.comprobante_numero ? ` ${g.comprobante_numero}` : ''}` : ''}
                    {g.usuario ? ` · registró ${g.usuario}` : ''}
                  </small>
                  {g.anulado && <small className="gst-motivo">Anulado: {g.motivo_anulacion}</small>}
                </div>
                <span className={`gst-metodo gst-metodo-${g.metodo_pago.toLowerCase()}`}>{etiquetaMetodo(g.metodo_pago, true)}</span>
                <strong className="gst-monto">{soles(g.monto)}</strong>
                {!g.anulado ? (
                  <button type="button" className="gst-anular" onClick={() => setVentana({ anular: g })} title="Anular gasto" aria-label={`Anular ${g.descripcion}`}>
                    <Ban size={15} />
                  </button>
                ) : (
                  <span className="gst-anular-vacio" />
                )}
              </div>
            ))}
          </div>
          <p className="gst-nota">
            La mercadería que compras para vender no va aquí: regístrala en Proveedores → Compras, así entra al stock y a su costo.
            Los gastos pagados con efectivo de la caja también se descuentan del cuadre de ese turno.
          </p>
        </>
      )}

      {ventana === 'NUEVO' && <FormularioGasto onCerrar={() => setVentana(null)} onGuardado={(r) => guardado(r.mensaje)} />}
      {ventana === 'CATEGORIAS' && <Categorias categorias={categorias} onCambio={recargar} onCerrar={() => setVentana(null)} />}
      {ventana?.anular && (
        <Anular gasto={ventana.anular} onCerrar={() => setVentana(null)} onAnulado={(texto) => guardado(texto)} />
      )}
    </div>
  );
}

function Anular({ gasto, onAnulado, onCerrar }) {
  const [motivo, setMotivo] = useState('');
  const [error, setError] = useState('');
  const [guardando, setGuardando] = useState(false);

  const anular = async (e) => {
    e.preventDefault();
    setGuardando(true);
    setError('');
    try {
      const r = await api.gastoAnular(gasto.id, motivo.trim());
      onAnulado(r.mensaje);
    } catch (err) {
      setError(err.message);
      setGuardando(false);
    }
  };

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="fg-modal" onSubmit={anular} role="dialog" aria-modal="true" aria-labelledby="gst-anular-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono gst-icono-peligro">
            <Ban size={18} />
          </span>
          <div>
            <h2 id="gst-anular-titulo">Anular gasto</h2>
            <p>
              {gasto.descripcion} · {soles(gasto.monto)}
            </p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>
        <div className="fg-cuerpo">
          <p className="fg-nota gst-nota-modal">
            No se borra: queda en la lista como anulado y deja de contar en los totales.
            {gasto.metodo_pago === 'EFECTIVO_CAJA' && ' Si su caja sigue abierta, el monto vuelve al efectivo esperado.'}
          </p>
          <label className="fg-campo">
            <span>¿Por qué se anula?</span>
            <input autoFocus value={motivo} maxLength={200} placeholder="Ej. Se registró dos veces" onChange={(e) => setMotivo(e.target.value)} />
          </label>
          {error && <p className="fg-error">{error}</p>}
        </div>
        <footer className="fg-acciones">
          <button type="button" className="fg-boton" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className="fg-boton gst-boton-peligro" disabled={!motivo.trim() || guardando}>
            {guardando ? 'Anulando…' : 'Anular gasto'}
          </button>
        </footer>
      </form>
    </div>
  );
}

function Categorias({ categorias, onCambio, onCerrar }) {
  const [nueva, setNueva] = useState('');
  const [editando, setEditando] = useState(null); // { id, nombre }
  const [error, setError] = useState('');
  const [ocupado, setOcupado] = useState(false);

  const hacer = async (accion) => {
    setOcupado(true);
    setError('');
    try {
      await accion();
      onCambio();
      return true;
    } catch (e) {
      setError(e.message);
      return false;
    } finally {
      setOcupado(false);
    }
  };

  const crear = async (e) => {
    e.preventDefault();
    if (!nueva.trim()) return;
    if (await hacer(() => api.gastoCategoriaCrear(nueva.trim()))) setNueva('');
  };

  const renombrar = async (e) => {
    e.preventDefault();
    if (!editando?.nombre.trim()) return;
    if (await hacer(() => api.gastoCategoriaActualizar(editando.id, { nombre: editando.nombre.trim() }))) setEditando(null);
  };

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="fg-modal" role="dialog" aria-modal="true" aria-labelledby="gst-cat-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono">
            <Tags size={18} />
          </span>
          <div>
            <h2 id="gst-cat-titulo">Categorías de gastos</h2>
            <p>Desactiva las que no uses: dejan de aparecer al registrar, pero los gastos anteriores se conservan.</p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>
        <div className="fg-cuerpo">
          <ul className="gst-cat-lista">
            {categorias.map((c) => (
              <li key={c.id} className={c.activo ? '' : 'gst-cat-inactiva'}>
                {editando?.id === c.id ? (
                  <form className="gst-cat-editar" onSubmit={renombrar}>
                    <input autoFocus value={editando.nombre} maxLength={60} onChange={(e) => setEditando({ ...editando, nombre: e.target.value })} />
                    <button type="submit" className="fg-boton fg-boton-principal" disabled={ocupado}>
                      Guardar
                    </button>
                    <button type="button" className="fg-boton" onClick={() => setEditando(null)}>
                      Cancelar
                    </button>
                  </form>
                ) : (
                  <>
                    <span>{c.nombre}</span>
                    <button type="button" className="gst-enlace" onClick={() => setEditando({ id: c.id, nombre: c.nombre })}>
                      Cambiar nombre
                    </button>
                    <button
                      type="button"
                      className="gst-enlace"
                      disabled={ocupado}
                      onClick={() => hacer(() => api.gastoCategoriaActualizar(c.id, { activo: !c.activo }))}
                    >
                      {c.activo ? 'Desactivar' : 'Activar'}
                    </button>
                  </>
                )}
              </li>
            ))}
          </ul>
          <form className="gst-cat-nueva" onSubmit={crear}>
            <input value={nueva} maxLength={60} placeholder="Nueva categoría, ej. Delivery" onChange={(e) => setNueva(e.target.value)} />
            <button type="submit" className="fg-boton fg-boton-principal" disabled={!nueva.trim() || ocupado}>
              Agregar
            </button>
          </form>
          {error && <p className="fg-error">{error}</p>}
        </div>
      </div>
    </div>
  );
}
