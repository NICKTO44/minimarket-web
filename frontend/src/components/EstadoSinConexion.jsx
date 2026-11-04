// Módulo "Venta sin conexión": aviso arriba de la pantalla y la lista de lo
// que se hizo sin internet (utils/sinConexion.js). Solo se muestra en un
// negocio con el módulo encendido, y solo cuando hay algo que decir.
import { useEffect, useRef, useState } from 'react';
import { CloudOff, RefreshCw, AlertTriangle, CheckCircle2, X, Download, Upload, Send, Trash2 } from 'lucide-react';
import {
  suscribirSinConexion,
  sincronizar,
  pendientesDe,
  deEsteNegocio,
  hayAtrasadas,
  descargarRespaldo,
  cargarRespaldo,
  descartarOperacion,
  guardarTelefono,
  listasParaEnviar,
  marcarEnviado,
  HORAS_AVISO,
} from '../utils/sinConexion';
import { enlaceComprobantePublico } from '../utils/enlacePublico';
import { confirmar } from '../utils/confirmar';
import './EstadoSinConexion.css';

const NOMBRE_TIPO = { VENTA: 'Venta', ABRIR_CAJA: 'Apertura de caja', GASTO: 'Gasto de caja' };

function textoEstado(op) {
  if (op.estado === 'HECHA') return 'Subida';
  if (op.estado === 'SIN_COMPROBANTE') return 'Subida, falta el comprobante';
  if (op.estado === 'POR_REVISAR') return 'Por revisar';
  return 'Por subir';
}

function fechaCorta(fecha) {
  // "2026-11-03 09:15:00" -> "03/11 09:15"
  if (!fecha) return '';
  return `${fecha.slice(8, 10)}/${fecha.slice(5, 7)} ${fecha.slice(11, 16)}`;
}

function nombreComprobante(c) {
  if (!c?.serie) return null;
  return `${c.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${c.serie}-${String(c.numero).padStart(6, '0')}`;
}

export default function EstadoSinConexion({ nombreTienda, identificadorNegocio }) {
  const [estado, setEstado] = useState(null);
  const [abierto, setAbierto] = useState(false);
  const [aviso, setAviso] = useState(null);
  const archivoRef = useRef(null);

  useEffect(() => suscribirSinConexion(setEstado), []);

  if (!estado) return null;
  const pendientes = pendientesDe(estado.operaciones);
  const porRevisar = pendientes.filter((o) => o.estado === 'POR_REVISAR');
  const porSubir = pendientes.filter((o) => o.estado !== 'POR_REVISAR');
  const atrasadas = hayAtrasadas(estado.operaciones);
  const recientes = deEsteNegocio(estado.operaciones);
  // Boletas o facturas ya emitidas de ventas sin internet, para mandarlas.
  const paraEnviar = listasParaEnviar(estado.operaciones);

  const mostrarBarra = !estado.enLinea || pendientes.length > 0 || estado.subiendo || estado.sinAlmacen || paraEnviar.length > 0;
  if (!mostrarBarra && !abierto) return null;

  const tono = atrasadas || porRevisar.length > 0 ? 'peligro' : !estado.enLinea ? 'aviso' : 'info';

  const subirAhora = async () => {
    setAviso(null);
    await sincronizar({ incluirPorRevisar: true });
  };

  const guardarRespaldo = () => {
    const n = descargarRespaldo(estado.operaciones, nombreTienda);
    setAviso(n > 0 ? `Respaldo descargado con ${n} operación(es) por subir.` : 'No hay nada por subir.');
  };

  const elegirRespaldo = async (e) => {
    const archivo = e.target.files?.[0];
    e.target.value = '';
    if (!archivo) return;
    try {
      const n = await cargarRespaldo(archivo);
      setAviso(n > 0 ? `Se agregaron ${n} operación(es) del respaldo.` : 'Ese respaldo no tiene nada nuevo.');
    } catch (err) {
      setAviso(err.message);
    }
  };

  const descartar = async (op) => {
    const ok = await confirmar({
      titulo: '¿Quitar esta operación sin subirla?',
      mensaje:
        op.tipo === 'VENTA'
          ? 'La venta no quedará en el sistema ni se emitirá su boleta o factura. Hazlo solo si ya la registraste de otra forma.'
          : 'No quedará en el sistema. Hazlo solo si ya la registraste de otra forma.',
      textoConfirmar: 'Quitar',
      icono: 'eliminar',
    });
    if (ok) await descartarOperacion(op.uuid);
  };

  const enviarWhatsapp = async (op, numeroEscrito) => {
    const numero = String(numeroEscrito || op.resumen?.telefono || '').replace(/\D/g, '');
    if (numero.length < 9) {
      setAviso('Escribe un número de WhatsApp válido (9 dígitos).');
      return;
    }
    if (numeroEscrito) await guardarTelefono(op.uuid, numeroEscrito);
    const c = op.resultado?.comprobante;
    const total = Number(op.resumen?.total || 0).toFixed(2);
    let texto;
    if (c?.comprobante_id && identificadorNegocio) {
      const url = enlaceComprobantePublico(identificadorNegocio, c.comprobante_id, c.clave_publica);
      const tipo = c.tipo === 'FACTURA' ? 'factura' : 'boleta';
      texto = `Hola! Aquí tienes tu ${tipo} ${c.serie}-${String(c.numero).padStart(6, '0')} por S/ ${total} (compra del ${fechaCorta(op.fecha_local).slice(0, 5)}).\n\nPuedes verla aquí: ${url}\n\n¡Gracias por tu compra!`;
    } else {
      texto = `Hola! Gracias por tu compra. Total: S/ ${total} — Venta ${op.resultado?.folio || op.resumen?.folio_provisional || ''}.`;
    }
    const conPais = numero.length === 9 ? `51${numero}` : numero;
    window.open(`https://wa.me/${conPais}?text=${encodeURIComponent(texto)}`, '_blank');
    if (c?.comprobante_id) await marcarEnviado(op.uuid);
  };

  const ordenadas = [...recientes].sort((a, b) => b.creada - a.creada);

  return (
    <>
      {mostrarBarra && (
        <div className={`esc-barra esc-${tono}`} role="status">
          <span className="esc-icono">
            {estado.subiendo ? (
              <RefreshCw size={16} className="esc-girando" />
            ) : !estado.enLinea ? (
              <CloudOff size={16} />
            ) : pendientes.length === 0 && paraEnviar.length > 0 ? (
              <Send size={16} />
            ) : (
              <AlertTriangle size={16} />
            )}
          </span>
          <span className="esc-texto">
            {!estado.enLinea && <strong>Sin internet. Puedes seguir vendiendo. </strong>}
            {estado.subiendo && <strong>Subiendo lo hecho sin internet… </strong>}
            {porSubir.length > 0 && <span>{porSubir.length} por subir. </span>}
            {porRevisar.length > 0 && <span className="esc-fuerte">{porRevisar.length} por revisar. </span>}
            {atrasadas && (
              <span className="esc-fuerte">
                Hay ventas de hace más de {HORAS_AVISO / 24} días sin enviar a SUNAT: conéctate a internet cuanto antes.{' '}
              </span>
            )}
            {paraEnviar.length > 0 && (
              <span>
                {paraEnviar.length === 1 ? '1 comprobante listo' : `${paraEnviar.length} comprobantes listos`} para enviar por WhatsApp.{' '}
              </span>
            )}
            {estado.sinAlmacen && (
              <span className="esc-fuerte">Este navegador no deja guardar datos: no cierres esta pestaña hasta que vuelva el internet. </span>
            )}
          </span>
          <button type="button" className="esc-boton" onClick={() => setAbierto(true)}>
            Ver
          </button>
        </div>
      )}

      {abierto && (
        <div className="esc-velo" onMouseDown={(e) => e.target === e.currentTarget && setAbierto(false)}>
          <div className="esc-panel" role="dialog" aria-modal="true" aria-labelledby="esc-titulo">
            <header className="esc-cabecera">
              <div>
                <h2 id="esc-titulo">Venta sin conexión</h2>
                <p>
                  {estado.enLinea ? 'Con internet.' : 'Sin internet.'} Lo hecho sin conexión se sube solo, en orden, cuando vuelve el internet.
                </p>
              </div>
              <button type="button" className="esc-cerrar" onClick={() => setAbierto(false)} aria-label="Cerrar">
                <X size={18} />
              </button>
            </header>

            <div className="esc-acciones">
              <button type="button" onClick={subirAhora} disabled={estado.subiendo || pendientes.length === 0}>
                <RefreshCw size={15} /> {estado.subiendo ? 'Subiendo…' : 'Subir ahora'}
              </button>
              <button type="button" onClick={guardarRespaldo} disabled={pendientes.length === 0}>
                <Download size={15} /> Guardar respaldo
              </button>
              <button type="button" onClick={() => archivoRef.current?.click()}>
                <Upload size={15} /> Cargar respaldo
              </button>
              <input ref={archivoRef} type="file" accept="application/json,.json" hidden onChange={elegirRespaldo} />
            </div>
            {aviso && <p className="esc-mensaje">{aviso}</p>}

            {ordenadas.length === 0 ? (
              <p className="esc-vacio">No hay nada hecho sin conexión.</p>
            ) : (
              <ul className="esc-lista">
                {ordenadas.map((op) => (
                  <FilaOperacion key={op.uuid} op={op} onReintentar={() => sincronizar({ incluirPorRevisar: true, solo: op.uuid })} onDescartar={() => descartar(op)} onWhatsapp={enviarWhatsapp} subiendo={estado.subiendo} />
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </>
  );
}

function FilaOperacion({ op, onReintentar, onDescartar, onWhatsapp, subiendo }) {
  const [telefono, setTelefono] = useState(op.resumen?.telefono || '');
  const comprobante = nombreComprobante(op.resultado?.comprobante);
  const folio = op.resultado?.folio;
  const hecha = op.estado === 'HECHA';
  return (
    <li className={`esc-fila esc-fila-${op.estado.toLowerCase()}`}>
      <div className="esc-fila-cabecera">
        <span className="esc-fila-tipo">
          {hecha ? <CheckCircle2 size={15} /> : op.estado === 'POR_REVISAR' ? <AlertTriangle size={15} /> : <CloudOff size={15} />}
          {NOMBRE_TIPO[op.tipo] || op.tipo}
          {op.resumen?.folio_provisional && <small> · {op.resumen.folio_provisional}</small>}
        </span>
        <span className="esc-fila-fecha">{fechaCorta(op.fecha_local)}</span>
      </div>
      <div className="esc-fila-detalle">
        {op.resumen?.total != null && <strong>S/ {Number(op.resumen.total).toFixed(2)}</strong>}
        {op.resumen?.descripcion && <span>{op.resumen.descripcion}</span>}
        {op.resumen?.cliente && <span>{op.resumen.cliente}</span>}
        {op.resumen?.comprobante && !comprobante && <span>{op.resumen.comprobante} pendiente</span>}
        <span className="esc-chip">{textoEstado(op)}</span>
      </div>
      {(folio || comprobante) && (
        <div className="esc-fila-resultado">
          {folio && <span>Venta {folio}</span>}
          {comprobante && <span>{comprobante} · {op.resultado.comprobante.estado === 'ACEPTADO' ? 'aceptada por SUNAT' : 'enviándose a SUNAT'}</span>}
        </div>
      )}
      {op.error && <p className="esc-fila-error">{op.error}</p>}
      {op.estado === 'POR_REVISAR' && (
        <div className="esc-fila-botones">
          <button type="button" onClick={onReintentar} disabled={subiendo}>
            <RefreshCw size={14} /> Reintentar
          </button>
          <button type="button" className="esc-peligro" onClick={onDescartar} disabled={subiendo}>
            <Trash2 size={14} /> Quitar
          </button>
        </div>
      )}
      {op.estado === 'SIN_COMPROBANTE' && (
        <div className="esc-fila-botones">
          <button type="button" onClick={onReintentar} disabled={subiendo}>
            <RefreshCw size={14} /> Emitir otra vez
          </button>
        </div>
      )}
      {hecha && op.tipo === 'VENTA' && (
        <div className="esc-fila-whatsapp">
          <input type="tel" placeholder="WhatsApp del cliente" value={telefono} onChange={(e) => setTelefono(e.target.value)} />
          <button type="button" onClick={() => onWhatsapp(op, telefono)}>
            <Send size={14} /> {op.resumen?.enviado ? 'Enviar otra vez' : 'Enviar por WhatsApp'}
          </button>
        </div>
      )}
    </li>
  );
}
