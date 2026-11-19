import { useCallback, useEffect, useState } from 'react';
import { api } from '../../api/api';
import './ConfiguracionCargos.css';

/** "2026-11-19 01:23:45" (UTC, como la base) -> "18/11 20:23" en Perú. */
function fechaLima(utc) {
  if (!utc) return '';
  const d = new Date(utc.replace(' ', 'T') + 'Z');
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleString('es-PE', { timeZone: 'America/Lima', day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
}

const NOMBRE_MODO = { YAPE: 'Celular del Yape', TIENDA: 'Celular de la tienda' };

/**
 * Configuración → Avisos de Yape y Plin (módulo AVISOS_PAGO): los celulares
 * vinculados con la app Monspeet (se vinculan desde la app, entrando con el
 * usuario administrador) y los pagos recibidos hoy.
 */
export default function ConfiguracionAvisos({ onMensaje }) {
  const [dispositivos, setDispositivos] = useState(null);
  const [pagos, setPagos] = useState([]);
  const [porQuitar, setPorQuitar] = useState(null);

  const cargar = useCallback(() => {
    Promise.all([api.avisosDispositivos(), api.avisosPagosHoy()])
      .then(([d, p]) => {
        setDispositivos(d || []);
        setPagos(p || []);
      })
      .catch((e) => onMensaje({ tipo: 'error', texto: e.message }));
  }, [onMensaje]);

  useEffect(() => {
    cargar();
  }, [cargar]);

  if (!dispositivos) return null;

  const quitar = async (d) => {
    try {
      await api.avisosDispositivoQuitar(d.id);
      setPorQuitar(null);
      onMensaje({ tipo: 'exito', texto: `"${d.nombre}" ya no recibe ni envía avisos.` });
      cargar();
    } catch (e) {
      onMensaje({ tipo: 'error', texto: e.message });
    }
  };

  return (
    <section className="cfg-card cc">
      <h3 className="cfg-subtitulo-seccion">Avisos de Yape y Plin</h3>
      <p className="cc-ayuda">
        Instala la app <b>Monspeet</b> en el celular donde llegan los Yape y en el celular de la tienda. En cada uno entra con el usuario
        administrador y elige si es el <b>celular del Yape</b> o el <b>de la tienda</b>. Cuando llega un Yape o Plin, el de la tienda lo dice
        en voz alta al instante, con el nombre, el monto y el código de seguridad.
      </p>

      {dispositivos.length === 0 ? (
        <p className="cc-ayuda">
          <b>Todavía no hay celulares vinculados.</b>
        </p>
      ) : (
        <div className="cc-lista">
          {dispositivos.map((d) => (
            <div key={d.id} className="cc-fila">
              <div className="cc-producto">
                <b>{d.nombre}</b>
                <small>
                  {NOMBRE_MODO[d.modo] || d.modo}
                  {d.ultimo_uso ? ` · conectado por última vez ${fechaLima(d.ultimo_uso)}` : ' · aún no se conecta'}
                </small>
              </div>
              {porQuitar === d.id ? (
                <>
                  <button type="button" className="cc-quitar" onClick={() => quitar(d)}>
                    Sí, quitar
                  </button>
                  <button type="button" className="cc-quitar" onClick={() => setPorQuitar(null)}>
                    No
                  </button>
                </>
              ) : (
                <button type="button" className="cc-quitar" onClick={() => setPorQuitar(d.id)}>
                  Quitar
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      <h4 className="cfg-subtitulo-seccion" style={{ fontSize: 14, marginTop: 16 }}>
        Pagos recibidos hoy{' '}
        <button type="button" className="cc-quitar" onClick={cargar}>
          Actualizar
        </button>
      </h4>
      {pagos.length === 0 ? (
        <p className="cc-ayuda">Aún no llega ningún pago hoy.</p>
      ) : (
        <div className="cc-lista">
          {pagos.map((p) => (
            <div key={p.id} className="cc-fila" style={p.reconocido ? undefined : { opacity: 0.6 }}>
              <div className="cc-producto">
                <b>
                  {p.reconocido ? '✅' : '⚠️'} {p.billetera === 'OTRO' ? 'Pago' : p.billetera === 'YAPE' ? 'Yape' : 'Plin'}
                  {p.monto != null ? ` S/ ${Number(p.monto).toFixed(2)}` : ''}
                  {p.nombre ? ` · ${p.nombre}` : ''}
                </b>
                <small>
                  {p.hora}
                  {p.codigo_seguridad ? ` · código ${p.codigo_seguridad}` : ''}
                  {p.reconocido ? '' : ` · no parece un pago recibido: "${p.texto}"`}
                </small>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
