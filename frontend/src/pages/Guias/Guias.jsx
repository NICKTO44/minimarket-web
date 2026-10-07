import { useCallback, useEffect, useState } from 'react';
import { FileDown, Printer, RefreshCw, Route, Settings } from 'lucide-react';
import { api } from '../../api/api';
import GuiaImprimible from '../../components/GuiaImprimible';
import SelectorUbigeo from '../../components/SelectorUbigeo';
import { fechaCorta } from '../../utils/formato';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import '../../components/PantallaModulo.css';

/** Hoy en el formato de <input type="date"> (hora del dispositivo). */
function hoyLocal() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function EstadoGuia({ estado }) {
  if (estado === 'ACEPTADA') return <span className="pm-chip pm-chip-ok">Aceptada</span>;
  if (estado === 'RECHAZADA') return <span className="pm-chip pm-chip-mal">Rechazada</span>;
  if (estado === 'ENVIADA') return <span className="pm-chip pm-chip-aviso">Esperando a SUNAT</span>;
  return <span className="pm-chip pm-chip-aviso">Sin enviar</span>;
}

const FORMULARIO_VACIO = {
  destinatario_tipo: 'RUC',
  destinatario_documento: '',
  destinatario_nombre: '',
  motivo: '01',
  motivo_descripcion: '',
  modo: 'PRIVADO',
  peso_total: '',
  bultos: '1',
  llegada_ubigeo: '',
  llegada_direccion: '',
  transportista_ruc: '',
  transportista_nombre: '',
  transportista_mtc: '',
  chofer_documento: '',
  chofer_nombres: '',
  chofer_apellidos: '',
  chofer_licencia: '',
  placa: '',
  observaciones: '',
};

/**
 * Guías de remisión (módulo GUIAS). La guía acompaña el traslado de lo
 * vendido. Se elige la venta, se completan destino y transporte, y el
 * sistema la emite por FacturaLibre. Si SUNAT tarda en responder queda
 * "Esperando a SUNAT" y se consulta de nuevo desde su detalle.
 *
 * Una guía aceptada se imprime en ticket de 80 mm (hecho aquí, con los datos
 * con que se emitió) o en hoja A4 (el PDF de FacturaLibre).
 */
export default function Guias({ esAdmin, nombreTienda, direccion, telefono, ruc }) {
  const [config, setConfig] = useState(null);
  const [guias, setGuias] = useState(null);
  const [mensaje, setMensaje] = useState(null);
  const [abierta, setAbierta] = useState(null);
  const [consultando, setConsultando] = useState(false);

  // Nueva guía
  const [nueva, setNueva] = useState(false);
  const [ventas, setVentas] = useState([]);
  const [folio, setFolio] = useState('');
  const [venta, setVenta] = useState(null);
  const [form, setForm] = useState(FORMULARIO_VACIO);
  const [partida, setPartida] = useState({ ubigeo: '', direccion: '' });
  const [fechaTraslado, setFechaTraslado] = useState(hoyLocal);
  const [errorForm, setErrorForm] = useState(null);
  const [emitiendo, setEmitiendo] = useState(false);

  // Datos de mi local
  const [editarLocal, setEditarLocal] = useState(false);
  const [local, setLocal] = useState({ serie: 'T001', ubigeo: '' });
  const [guardandoLocal, setGuardandoLocal] = useState(false);

  const cargar = useCallback(() => {
    api
      .guias()
      .then(setGuias)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }));
  }, []);

  useEffect(() => {
    api
      .guiasConfig()
      .then(setConfig)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }));
    cargar();
  }, [cargar]);

  // Abre el detalle de una guía. La lista no trae lo necesario para el ticket
  // (transporte, ítems, QR): se pide aparte. Si el servidor aún no lo entrega,
  // la guía se abre igual, solo sin el botón del ticket.
  const abrir = (g) => {
    setAbierta(g);
    api
      .guia(g.id)
      .then((detalle) => setAbierta((actual) => (actual && actual.id === detalle.id ? { ...actual, ...detalle } : actual)))
      .catch(() => {});
  };

  const campo = (nombre) => ({ value: form[nombre], onChange: (e) => setForm((f) => ({ ...f, [nombre]: e.target.value })) });

  const abrirNueva = () => {
    const ultimo = config?.ultimo || {};
    setForm({
      ...FORMULARIO_VACIO,
      modo: ultimo.modo === 'PUBLICO' ? 'PUBLICO' : 'PRIVADO',
      transportista_ruc: ultimo.transportista?.ruc || '',
      transportista_nombre: ultimo.transportista?.nombre || '',
      transportista_mtc: ultimo.transportista?.mtc || '',
      chofer_documento: ultimo.chofer?.documento || '',
      chofer_nombres: ultimo.chofer?.nombres || '',
      chofer_apellidos: ultimo.chofer?.apellidos || '',
      chofer_licencia: ultimo.chofer?.licencia || '',
      placa: ultimo.placa || '',
    });
    setPartida({ ubigeo: config?.ubigeo || '', direccion: config?.direccion || '' });
    setFechaTraslado(hoyLocal());
    setVenta(null);
    setFolio('');
    setErrorForm(null);
    setNueva(true);
    // Ventas recientes para elegir sin escribir el folio.
    api
      .comprobantesListar()
      .then((lista) => setVentas(lista.slice(0, 60)))
      .catch(() => setVentas([]));
  };

  const elegirVenta = async (folioElegido) => {
    setFolio(folioElegido);
    setErrorForm(null);
    if (!folioElegido.trim()) {
      setVenta(null);
      return;
    }
    try {
      const v = await api.guiaVenta(folioElegido.trim());
      setVenta(v);
      setForm((f) => ({
        ...f,
        destinatario_tipo: v.destinatario_tipo === 'DNI' ? 'DNI' : v.destinatario_tipo === 'RUC' ? 'RUC' : f.destinatario_tipo,
        destinatario_documento: v.destinatario_documento || '',
        destinatario_nombre: v.destinatario_nombre || '',
        llegada_direccion: v.destinatario_direccion || '',
      }));
    } catch (e) {
      setVenta(null);
      setErrorForm(e.message);
    }
  };

  const emitir = async (e) => {
    e.preventDefault();
    if (!venta) {
      setErrorForm('Elige la venta que vas a despachar.');
      return;
    }
    setEmitiendo(true);
    setErrorForm(null);
    const privado = form.modo === 'PRIVADO';
    try {
      const guia = await api.guiaCrear({
        venta_id: venta.venta_id,
        destinatario_tipo: form.destinatario_tipo,
        destinatario_documento: form.destinatario_documento,
        destinatario_nombre: form.destinatario_nombre,
        motivo: form.motivo,
        motivo_descripcion: form.motivo_descripcion || null,
        modo: form.modo,
        fecha_traslado: fechaTraslado,
        peso_total: Number(String(form.peso_total).replace(',', '.')) || 0,
        bultos: parseInt(form.bultos, 10) || 0,
        partida,
        llegada: { ubigeo: form.llegada_ubigeo, direccion: form.llegada_direccion },
        transportista: privado ? null : { ruc: form.transportista_ruc, nombre: form.transportista_nombre, mtc: form.transportista_mtc || null },
        chofer: privado
          ? { documento: form.chofer_documento, nombres: form.chofer_nombres, apellidos: form.chofer_apellidos, licencia: form.chofer_licencia }
          : null,
        placa: privado ? form.placa : null,
        observaciones: form.observaciones || null,
        items: [],
      });
      setNueva(false);
      // Queda abierta para imprimirla (o para consultar a SUNAT si aún no responde).
      setAbierta(guia);
      setMensaje(
        guia.estado === 'ACEPTADA'
          ? { tipo: 'exito', texto: `Guía ${guia.numero} aceptada por SUNAT.` }
          : guia.estado === 'RECHAZADA'
            ? { tipo: 'error', texto: `Guía ${guia.numero} rechazada: ${guia.mensaje || ''}` }
            : { tipo: 'aviso', texto: `Guía ${guia.numero} creada. SUNAT aún no responde: ábrela y toca “Consultar SUNAT” en unos minutos.` }
      );
      cargar();
      api.guiasConfig().then(setConfig).catch(() => {});
    } catch (err) {
      setErrorForm(err.message);
    } finally {
      setEmitiendo(false);
    }
  };

  const consultar = async () => {
    setConsultando(true);
    try {
      const g = await api.guiaConsultar(abierta.id);
      setAbierta(g);
      cargar();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
      setAbierta(null);
    } finally {
      setConsultando(false);
    }
  };

  const guardarLocal = async () => {
    setGuardandoLocal(true);
    try {
      setConfig(await api.guiasConfigGuardar(local));
      setEditarLocal(false);
      setMensaje({ tipo: 'exito', texto: 'Datos de tu local guardados.' });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
      setEditarLocal(false);
    } finally {
      setGuardandoLocal(false);
    }
  };

  const faltaLocal = config && (!config.ubigeo || !config.direccion);
  const listo = config && config.facturacion_lista && !faltaLocal;

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <Route size={22} /> Guías de remisión
          </h1>
          <p className="pm-subtitulo">La guía electrónica que acompaña el traslado de lo que vendiste.</p>
        </div>
        <div className="pm-acciones">
          {esAdmin && config && (
            <button
              className="pm-boton-secundario"
              onClick={() => {
                setLocal({ serie: config.serie, ubigeo: config.ubigeo });
                setEditarLocal(true);
              }}
            >
              <Settings size={15} /> Datos de mi local
            </button>
          )}
          <button className="pm-boton" onClick={abrirNueva} disabled={!listo}>
            Nueva guía
          </button>
        </div>
      </header>

      {mensaje && <p className={`pm-mensaje pm-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}
      {config && !config.facturacion_lista && (
        <p className="pm-mensaje pm-mensaje-aviso">
          Para emitir guías primero configura FacturaLibre (Token y URL) en Configuración → Datos del negocio.
        </p>
      )}
      {faltaLocal && (
        <p className="pm-mensaje pm-mensaje-aviso">
          {!config.direccion
            ? 'Falta la dirección de tu negocio (Configuración → Datos del negocio). '
            : 'Falta indicar el distrito de tu local, que es el punto de partida de las guías. '}
          {esAdmin ? 'Toca “Datos de mi local”.' : 'Pídeselo al administrador.'}
        </p>
      )}

      <div className="pm-tarjeta">
        {!guias ? (
          <p className="pm-vacio">Cargando...</p>
        ) : guias.length === 0 ? (
          <p className="pm-vacio">Todavía no has emitido guías.</p>
        ) : (
          guias.map((g) => (
            <button key={g.id} className="pm-fila" onClick={() => abrir(g)}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">
                  {g.numero} · {g.destinatario_nombre} <EstadoGuia estado={g.estado} />
                </span>
                <span className="pm-fila-detalle">
                  Traslado {fechaCorta(g.fecha_traslado)} · a {g.llegada_direccion}
                  {g.folio_venta ? ` · venta ${g.folio_venta}` : ''}
                </span>
              </span>
            </button>
          ))
        )}
      </div>

      {abierta && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setAbierta(null)}>
          <div className="pm-modal" role="dialog" aria-modal="true" aria-label="Guía de remisión">
            <div className="pm-modal-cabecera">
              <h2>
                Guía {abierta.numero} <EstadoGuia estado={abierta.estado} />
              </h2>
              <p>Emitida el {fechaCorta(abierta.fecha)} por {abierta.usuario}</p>
            </div>
            <div className="pm-modal-cuerpo">
              <dl className="pm-pares">
                <dt>Destinatario</dt>
                <dd>
                  {abierta.destinatario_nombre} · {abierta.destinatario_documento}
                </dd>
                <dt>Llegada</dt>
                <dd>{abierta.llegada_direccion}</dd>
                <dt>Traslado</dt>
                <dd>{fechaCorta(abierta.fecha_traslado)}</dd>
                {abierta.folio_venta && (
                  <>
                    <dt>Venta</dt>
                    <dd>{abierta.folio_venta}</dd>
                  </>
                )}
                {abierta.mensaje && (
                  <>
                    <dt>SUNAT</dt>
                    <dd>{abierta.mensaje}</dd>
                  </>
                )}
              </dl>
              {abierta.estado !== 'ACEPTADA' && (
                <p className="pm-mensaje pm-mensaje-aviso">
                  {abierta.estado === 'RECHAZADA'
                    ? 'SUNAT la rechazó. Revisa el motivo; si era un problema pasajero puedes reintentar, y si los datos estaban mal, emite una guía nueva.'
                    : 'SUNAT puede tardar unos minutos en responder. Vuelve a consultar hasta que salga aceptada.'}
                </p>
              )}
            </div>
            <div className="pm-modal-pie">
              {abierta.estado !== 'ACEPTADA' && (
                <button className="pm-boton" onClick={consultar} disabled={consultando}>
                  <RefreshCw size={15} /> {consultando ? 'Consultando...' : 'Consultar SUNAT'}
                </button>
              )}
              {abierta.estado === 'ACEPTADA' && abierta.datos && (
                <button className="pm-boton" onClick={() => window.print()}>
                  <Printer size={15} /> Ticket 80 mm
                </button>
              )}
              {abierta.enlace_pdf && (
                <a className="pm-boton-secundario" href={abierta.enlace_pdf} target="_blank" rel="noreferrer" style={{ textDecoration: 'none' }}>
                  <FileDown size={15} /> Hoja A4
                </a>
              )}
              <button className="pm-boton-secundario" onClick={() => setAbierta(null)}>
                Cerrar
              </button>
            </div>
          </div>
        </div>
      )}

      {nueva && (
        <div className="pm-velo">
          <form className="pm-modal pm-modal-ancho" onSubmit={emitir} aria-label="Nueva guía de remisión">
            <div className="pm-modal-cabecera">
              <h2>Nueva guía de remisión</h2>
              <p>Serie {config.serie} · sale de {config.direccion}</p>
            </div>
            <div className="pm-modal-cuerpo">
              <h3 className="pm-seccion">1. Venta que se despacha</h3>
              <div className="pm-campos">
                <label className="pm-campo pm-campo-ancho">
                  <span>Venta</span>
                  <select value={ventas.some((v) => v.folio_venta === folio) ? folio : ''} onChange={(e) => elegirVenta(e.target.value)}>
                    <option value="">Elige una venta reciente...</option>
                    {ventas.map((v) => (
                      <option key={v.venta_id} value={v.folio_venta}>
                        {v.folio_venta} · {v.cliente_nombre || 'Sin cliente'} · S/ {Number(v.monto).toFixed(2)}
                      </option>
                    ))}
                  </select>
                  <small>
                    ¿No está en la lista? Escribe su folio:{' '}
                    <input
                      className="guia-folio"
                      placeholder="V-20261003-0001"
                      value={folio}
                      onChange={(e) => setFolio(e.target.value)}
                      onBlur={(e) => e.target.value.trim() && e.target.value !== venta?.folio && elegirVenta(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault();
                          elegirVenta(folio);
                        }
                      }}
                      aria-label="Folio de la venta"
                    />
                  </small>
                </label>
              </div>
              {venta && (
                <>
                  <table className="pm-lineas" style={{ marginTop: 8 }}>
                    <tbody>
                      {venta.items.map((it, i) => (
                        <tr key={i}>
                          <td>{it.descripcion}</td>
                          <td className="pm-num">
                            {formatoCantidad(it.cantidad)} {abreviaturaUnidad(it.unidad)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  <p className="pm-subtitulo">
                    {venta.items.length === 0
                      ? 'Esta venta solo tiene servicios: no hay nada que trasladar.'
                      : `Va todo lo de la venta${venta.comprobante ? ` (comprobante ${venta.comprobante})` : ''}. Los servicios no se incluyen.`}
                    {venta.guias.length > 0 && ` Ojo: esta venta ya tiene ${venta.guias.length} guía(s): ${venta.guias.map((g) => g.numero).join(', ')}.`}
                  </p>
                </>
              )}

              <h3 className="pm-seccion">2. Quién recibe</h3>
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Documento</span>
                  <select {...campo('destinatario_tipo')}>
                    <option value="RUC">RUC</option>
                    <option value="DNI">DNI</option>
                    <option value="CE">Carnet de extranjería</option>
                  </select>
                </label>
                <label className="pm-campo">
                  <span>Número</span>
                  <input inputMode="numeric" {...campo('destinatario_documento')} />
                </label>
                <label className="pm-campo pm-campo-ancho">
                  <span>Nombre o razón social</span>
                  <input {...campo('destinatario_nombre')} />
                </label>
              </div>

              <h3 className="pm-seccion">3. Traslado</h3>
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Motivo</span>
                  <select {...campo('motivo')}>
                    {config.motivos.map((m) => (
                      <option key={m.codigo} value={m.codigo}>
                        {m.nombre}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="pm-campo">
                  <span>Fecha de traslado</span>
                  <input type="date" min={hoyLocal()} value={fechaTraslado} onChange={(e) => setFechaTraslado(e.target.value)} />
                </label>
                {form.motivo === '13' && (
                  <label className="pm-campo pm-campo-ancho">
                    <span>Describe el motivo</span>
                    <input {...campo('motivo_descripcion')} />
                  </label>
                )}
                <label className="pm-campo">
                  <span>Peso total (kg)</span>
                  <input inputMode="decimal" placeholder="850" {...campo('peso_total')} />
                </label>
                <label className="pm-campo">
                  <span>Bultos</span>
                  <input inputMode="numeric" {...campo('bultos')} />
                </label>
                <div className="pm-campo">
                  <span>Sale de (distrito)</span>
                  <SelectorUbigeo valor={partida.ubigeo} onCambiar={(u) => setPartida((p) => ({ ...p, ubigeo: u }))} etiqueta="Distrito de partida" />
                </div>
                <label className="pm-campo">
                  <span>Dirección de partida</span>
                  <input value={partida.direccion} onChange={(e) => setPartida((p) => ({ ...p, direccion: e.target.value }))} />
                </label>
                <div className="pm-campo">
                  <span>Llega a (distrito)</span>
                  <SelectorUbigeo valor={form.llegada_ubigeo} onCambiar={(u) => setForm((f) => ({ ...f, llegada_ubigeo: u }))} etiqueta="Distrito de llegada" />
                </div>
                <label className="pm-campo">
                  <span>Dirección de llegada</span>
                  <input {...campo('llegada_direccion')} />
                </label>
              </div>

              <h3 className="pm-seccion">4. Transporte</h3>
              <div className="pm-opciones" role="radiogroup" aria-label="Tipo de transporte">
                <button type="button" role="radio" aria-checked={form.modo === 'PRIVADO'} className={form.modo === 'PRIVADO' ? 'activo' : ''} onClick={() => setForm((f) => ({ ...f, modo: 'PRIVADO' }))}>
                  Vehículo propio
                </button>
                <button type="button" role="radio" aria-checked={form.modo === 'PUBLICO'} className={form.modo === 'PUBLICO' ? 'activo' : ''} onClick={() => setForm((f) => ({ ...f, modo: 'PUBLICO' }))}>
                  Empresa de transporte
                </button>
              </div>
              {form.modo === 'PRIVADO' ? (
                <div className="pm-campos" style={{ marginTop: 10 }}>
                  <label className="pm-campo">
                    <span>Placa del vehículo</span>
                    <input placeholder="A1Y298" {...campo('placa')} />
                  </label>
                  <label className="pm-campo">
                    <span>DNI del conductor</span>
                    <input inputMode="numeric" maxLength={8} {...campo('chofer_documento')} />
                  </label>
                  <label className="pm-campo">
                    <span>Nombres</span>
                    <input {...campo('chofer_nombres')} />
                  </label>
                  <label className="pm-campo">
                    <span>Apellidos</span>
                    <input {...campo('chofer_apellidos')} />
                  </label>
                  <label className="pm-campo pm-campo-ancho">
                    <span>Licencia de conducir</span>
                    <input placeholder="Q41784439" {...campo('chofer_licencia')} />
                  </label>
                </div>
              ) : (
                <div className="pm-campos" style={{ marginTop: 10 }}>
                  <label className="pm-campo">
                    <span>RUC del transportista</span>
                    <input inputMode="numeric" maxLength={11} {...campo('transportista_ruc')} />
                  </label>
                  <label className="pm-campo">
                    <span>Registro MTC (opcional)</span>
                    <input {...campo('transportista_mtc')} />
                  </label>
                  <label className="pm-campo pm-campo-ancho">
                    <span>Razón social</span>
                    <input {...campo('transportista_nombre')} />
                  </label>
                </div>
              )}
              <div className="pm-campos" style={{ marginTop: 10 }}>
                <label className="pm-campo pm-campo-ancho">
                  <span>Observaciones (opcional)</span>
                  <input maxLength={250} {...campo('observaciones')} />
                </label>
              </div>
              {errorForm && <p className="pm-mensaje pm-mensaje-error" style={{ marginTop: 12 }}>{errorForm}</p>}
            </div>
            <div className="pm-modal-pie">
              <button type="button" className="pm-boton-secundario" onClick={() => setNueva(false)} disabled={emitiendo}>
                Cancelar
              </button>
              <button type="submit" className="pm-boton" disabled={emitiendo || !venta || venta.items.length === 0}>
                {emitiendo ? 'Emitiendo...' : 'Emitir guía'}
              </button>
            </div>
          </form>
        </div>
      )}

      {editarLocal && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setEditarLocal(false)}>
          <div className="pm-modal pm-modal-chico" role="dialog" aria-modal="true" aria-label="Datos de mi local">
            <div className="pm-modal-cabecera">
              <h2>Datos de mi local</h2>
              <p>Es el punto de partida de tus guías. La dirección se cambia en Configuración → Datos del negocio.</p>
            </div>
            <div className="pm-modal-cuerpo">
              <dl className="pm-pares">
                <dt>Dirección</dt>
                <dd>{config.direccion || 'Sin dirección'}</dd>
              </dl>
              <div className="pm-campos">
                <div className="pm-campo pm-campo-ancho">
                  <span>Distrito del local</span>
                  <SelectorUbigeo valor={local.ubigeo} onCambiar={(u) => setLocal((l) => ({ ...l, ubigeo: u }))} etiqueta="Distrito del local" />
                </div>
                <label className="pm-campo pm-campo-ancho">
                  <span>Serie de guías</span>
                  <input maxLength={4} value={local.serie} onChange={(e) => setLocal((l) => ({ ...l, serie: e.target.value.toUpperCase() }))} />
                  <small>La que tienes en FacturaLibre para guías de remitente (empieza con T, por ejemplo T001).</small>
                </label>
              </div>
            </div>
            <div className="pm-modal-pie">
              <button className="pm-boton-secundario" onClick={() => setEditarLocal(false)}>
                Cancelar
              </button>
              <button className="pm-boton" onClick={guardarLocal} disabled={guardandoLocal || !local.ubigeo || local.serie.length !== 4}>
                {guardandoLocal ? 'Guardando...' : 'Guardar'}
              </button>
            </div>
          </div>
        </div>
      )}

      <GuiaImprimible
        guia={abierta?.estado === 'ACEPTADA' ? abierta : null}
        motivos={config?.motivos}
        nombreTienda={nombreTienda}
        direccion={direccion}
        telefono={telefono}
        ruc={ruc}
      />
    </div>
  );
}
