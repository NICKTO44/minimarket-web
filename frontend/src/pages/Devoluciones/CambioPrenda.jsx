// Cambio de prenda (módulo "Cambio de prenda", rubro Ropa y calzado).
// Aquí se busca la venta y se marca lo que el cliente devuelve. Lo que se
// lleva se elige en el punto de venta, que cobra o devuelve solo la diferencia.
import { useState } from 'react';
import { api } from '../../api/api';
import { fechaCorta, fechaHoraLima } from '../../utils/formato';

const redondear2 = (n) => Math.round(n * 100) / 100;

export default function CambioPrenda({ onIniciarCambio }) {
  const [busqueda, setBusqueda] = useState('');
  const [buscando, setBuscando] = useState(false);
  const [venta, setVenta] = useState(null);
  // detalle_id -> { cantidad (texto), con_falla }
  const [elegidos, setElegidos] = useState({});
  const [motivo, setMotivo] = useState('');
  const [mensaje, setMensaje] = useState(null);
  // Emisión directa: el cajero decide si sale la nota de crédito de lo devuelto.
  const [conNota, setConNota] = useState(false);

  const buscar = async () => {
    if (!busqueda.trim()) return;
    setMensaje(null);
    setVenta(null);
    setBuscando(true);
    try {
      const resultado = await api.cambioVenta(busqueda.trim());
      setVenta(resultado);
      setConNota(false);
      // Una sola prenda en la venta: lo normal es que sea esa la que cambia.
      const cambiables = resultado.productos.filter((p) => p.disponible > 0);
      setElegidos(
        cambiables.length === 1 && cambiables[0].disponible === 1
          ? { [cambiables[0].detalle_id]: { cantidad: '1', con_falla: false } }
          : {}
      );
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setBuscando(false);
    }
  };

  const cambiar = (detalleId, cambios) => {
    setElegidos((prev) => ({ ...prev, [detalleId]: { cantidad: '0', con_falla: false, ...prev[detalleId], ...cambios } }));
  };

  const items = (venta?.productos || [])
    .map((p) => {
      const elegido = elegidos[p.detalle_id];
      const cantidad = parseFloat(elegido?.cantidad) || 0;
      return { ...p, cantidad_a_cambiar: cantidad, con_falla: !!elegido?.con_falla };
    })
    .filter((p) => p.cantidad_a_cambiar > 0);
  const valor = redondear2(items.reduce((s, p) => s + redondear2(p.valor_unitario * p.cantidad_a_cambiar), 0));
  const pasado = items.find((p) => p.cantidad_a_cambiar > p.disponible);

  const continuar = () => {
    setMensaje(null);
    if (items.length === 0) {
      setMensaje({ tipo: 'error', texto: 'Marca qué prenda devuelve el cliente.' });
      return;
    }
    if (pasado) {
      setMensaje({ tipo: 'error', texto: `De "${pasado.nombre}" solo se puede cambiar ${pasado.disponible}.` });
      return;
    }
    onIniciarCambio({
      venta_id: venta.venta_id,
      folio: venta.folio,
      valor,
      motivo: motivo.trim() || null,
      emitir_nota_credito: !!venta.nota_credito_posible && conNota,
      items: items.map((p) => ({
        detalle_id: p.detalle_id,
        nombre: p.nombre,
        cantidad: p.cantidad_a_cambiar,
        valor_unitario: p.valor_unitario,
        con_falla: p.con_falla,
      })),
    });
  };

  return (
    <>
      <p className="dev-subtitulo">
        Busca la venta por su folio (ej: V-20260822-0001) o por el número de boleta o factura que trae el cliente. Marca
        lo que devuelve y después eliges lo que se lleva: solo se cobra o se devuelve la diferencia.
      </p>

      <div className="dev-buscador">
        <input
          type="text"
          placeholder="Folio de venta o número de comprobante..."
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              buscar();
            }
          }}
        />
        <button onClick={buscar} disabled={buscando}>
          {buscando ? 'Buscando...' : 'Buscar'}
        </button>
      </div>

      {mensaje && <p className={`dev-mensaje dev-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {venta && (
        <div className="dev-venta-card">
          <div className="dev-venta-header">
            <div>
              <span className="dev-venta-folio">{venta.folio}</span>
              {venta.comprobante && (
                <span className="dev-venta-comprobante">
                  {venta.comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} {venta.comprobante.serie}-
                  {String(venta.comprobante.numero).padStart(6, '0')}
                </span>
              )}
            </div>
            <span className="dev-venta-fecha">{fechaHoraLima(venta.fecha_hora)}</span>
          </div>
          <div className="dev-venta-total">Total de la venta: S/ {venta.total.toFixed(2)}</div>

          {venta.al_credito ? (
            <p className="dev-mensaje dev-mensaje-error">
              Esta venta fue al crédito. Registra la devolución en la pestaña «Devolución» (se descuenta de la deuda) y
              luego haz la venta nueva.
            </p>
          ) : (
            <>
              {venta.fuera_de_plazo && (
                <p className="dev-cambio-aviso">
                  Esta venta ya pasó el plazo de cambio de {venta.plazo_dias} días
                  {venta.limite ? ` (venció el ${fechaCorta(venta.limite)})` : ''}. Puedes continuar si el negocio lo
                  autoriza.
                </p>
              )}

              <div className="dev-items">
                {venta.productos.map((p) => {
                  const elegido = elegidos[p.detalle_id] || { cantidad: '0', con_falla: false };
                  const cantidad = parseFloat(elegido.cantidad) || 0;
                  return (
                    <div key={p.detalle_id} className={`dev-item${p.disponible <= 0 ? ' dev-item-agotado' : ''}`}>
                      <div className="dev-item-info">
                        <span className="dev-item-nombre">{p.nombre}</span>
                        <span className="dev-item-detalle">
                          Pagó S/ {p.valor_unitario.toFixed(2)} c/u ·{' '}
                          {p.disponible <= 0
                            ? 'ya se devolvió o se cambió'
                            : p.disponible < p.cantidad
                              ? `quedan ${p.disponible} de ${p.cantidad} por cambiar`
                              : `compró ${p.cantidad}`}
                        </span>
                      </div>
                      {p.disponible > 0 && (
                        <div className="dev-item-controles dev-cambio-controles">
                          {p.disponible === 1 ? (
                            <button
                              type="button"
                              className={`dev-cambio-marcar${cantidad > 0 ? ' activo' : ''}`}
                              onClick={() => cambiar(p.detalle_id, { cantidad: cantidad > 0 ? '0' : '1' })}
                              aria-pressed={cantidad > 0}
                            >
                              {cantidad > 0 ? '✓ La devuelve' : 'La devuelve'}
                            </button>
                          ) : (
                            <input
                              type="number"
                              min="0"
                              max={p.disponible}
                              placeholder="Cant."
                              value={elegido.cantidad}
                              onChange={(e) => cambiar(p.detalle_id, { cantidad: e.target.value })}
                              aria-label={`Cantidad que devuelve de ${p.nombre}`}
                            />
                          )}
                          {cantidad > 0 && (
                            <label className="dev-cambio-falla">
                              <input
                                type="checkbox"
                                checked={elegido.con_falla}
                                onChange={(e) => cambiar(p.detalle_id, { con_falla: e.target.checked })}
                              />
                              Tiene falla (no vuelve a la venta)
                            </label>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>

              <div className="dev-campo">
                <label>Motivo (opcional)</label>
                <input
                  value={motivo}
                  onChange={(e) => setMotivo(e.target.value)}
                  placeholder="Ej: no le quedó la talla, quiere otro color, tenía una falla..."
                  maxLength={200}
                />
              </div>

              {venta.nota_credito_posible && (
                <label className="dev-cambio-nota">
                  <input type="checkbox" checked={conNota} onChange={(e) => setConNota(e.target.checked)} />
                  <span>
                    <strong>Emitir nota de crédito de lo que devuelve</strong>
                    <small>
                      Ante SUNAT lo correcto es la nota por lo devuelto y una boleta nueva por lo que se lleva. Si solo cambia la
                      talla o el color al mismo precio, puedes no emitirla.
                    </small>
                  </span>
                </label>
              )}

              <div className="dev-total-row dev-cambio-total">
                <span>A favor del cliente</span>
                <strong>S/ {valor.toFixed(2)}</strong>
              </div>

              <button className="dev-boton-procesar dev-cambio-boton" onClick={continuar} disabled={items.length === 0}>
                Elegir lo que se lleva →
              </button>
            </>
          )}
        </div>
      )}
    </>
  );
}
