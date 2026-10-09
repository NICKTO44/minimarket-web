import { useState, useEffect } from 'react';
import QRCode from 'qrcode';
import { montoEnLetras } from '../utils/numeroALetras';
import { construirCadenaQrSunat } from '../utils/qrSunat';
import { nombreMetodo } from '../utils/metodoPago';
import { desgloseDeComprobante, etiquetaTasa } from '../utils/igv';
import { formatoCantidad, subtotalLinea } from '../utils/medidas';

// diasCambio: plazo para cambiar una prenda (módulo "Cambio de prenda");
// 0 o sin el módulo = no se imprime nada.
// fecha: fecha y hora de emisión ya escritas (al reimprimir); sin ella, la
// de este momento (la venta se acaba de hacer).
export default function Recibo({
  venta,
  items,
  nombreTienda,
  // Razón social (emisión directa): va debajo del nombre comercial cuando
  // es distinta, como pide SUNAT en la representación impresa.
  razonSocial = null,
  direccion,
  telefono,
  ruc,
  cajero,
  comprobante,
  cliente,
  diasCambio = 0,
  fecha = null,
}) {
  const esComprobanteReal = !!comprobante;
  // La factura también sale en ticket de 80 mm: lleva el RUC del cliente,
  // la forma de pago y la leyenda de representación impresa.
  const esFactura = comprobante?.tipo === 'FACTURA';
  const formaPago = venta.credito || venta.metodoPago === 'CREDITO' ? 'CRÉDITO' : 'CONTADO';
  const encabezado = comprobante
    ? `${comprobante.tipo === 'FACTURA' ? 'FACTURA ELECTRÓNICA' : 'BOLETA DE VENTA ELECTRÓNICA'}`
    : 'NOTA DE VENTA (sin comprobante tributario)';
  const numeroDocumento = comprobante ? `${comprobante.serie}-${String(comprobante.numero).padStart(6, '0')}` : null;

  // Se usan los valores REALES devueltos por el backend (los mismos que
  // se firmaron con FacturaLibre) cuando existen; si no (nota simple, o
  // un comprobante viejo del historial sin estos campos todavía), se
  // recalcula localmente solo para mostrar el desglose visual — nunca
  // para el QR, que solo se dibuja si hay datos reales completos.
  const total = comprobante?.total_venta ?? venta.total;
  // Tasa del negocio y totales por tipo de operación (gravado, exonerado,
  // inafecto), tal como se enviaron a SUNAT.
  const desglose = desgloseDeComprobante(comprobante, total);
  const igv = desglose.igv;

  const totalUnidades = formatoCantidad(items.reduce((sum, item) => sum + item.cantidad, 0));

  // Cambio de prenda: lo que el cliente devolvió y la diferencia.
  const cambioPrenda = venta.cambioPrenda || null;
  // Último día para cambiar lo comprado hoy.
  const limiteCambio = (() => {
    if (!(diasCambio > 0)) return null;
    const fecha = new Date();
    fecha.setDate(fecha.getDate() + diasCambio);
    return fecha.toLocaleDateString('es-PE');
  })();

  // Factura sujeta a detracción (SPOT): lo que se envió a SUNAT.
  const detraccion = comprobante?.detraccion_monto > 0 ? comprobante : null;

  // --- QR oficial de SUNAT ---
  // Solo se genera si tenemos TODOS los datos reales necesarios (RUC
  // emisor, hash, fecha de emisión). Un comprobante viejo reimpreso
  // desde el historial puede no traerlos todavía — en ese caso, el
  // ticket se imprime igual, simplemente sin QR, en vez de mostrar algo
  // roto o inventado.
  const [qrDataUrl, setQrDataUrl] = useState(null);

  const puedeGenerarQr =
    esComprobanteReal && !!comprobante?.ruc_emisor && !!comprobante?.hash && !!comprobante?.fecha_emision;

  useEffect(() => {
    if (!puedeGenerarQr) {
      setQrDataUrl(null);
      return;
    }

    const cadena = construirCadenaQrSunat({
      ruc: comprobante.ruc_emisor,
      tipoDocumento: comprobante.tipo === 'FACTURA' ? '01' : '03',
      serie: comprobante.serie,
      numero: comprobante.numero,
      igv,
      total,
      fechaEmision: comprobante.fecha_emision,
      tipoDocCliente: comprobante.cliente_tipo_documento_codigo || '0',
      numDocCliente: comprobante.cliente_numero_documento || '-',
      hash: comprobante.hash,
    });

    // Con `scale` cada cuadro del QR mide un número entero de píxeles (con
    // `width` salían bordes grises, que la ticketera imprime borrosos).
    QRCode.toDataURL(cadena, { margin: 0, scale: 6 })
      .then(setQrDataUrl)
      .catch(() => setQrDataUrl(null));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [puedeGenerarQr, comprobante?.hash, comprobante?.serie, comprobante?.numero]);

  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {razonSocial && razonSocial.trim() !== String(nombreTienda || '').trim() && (
        <div className="recibo-centro recibo-dato-tienda">{razonSocial}</div>
      )}
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
      {ruc && <div className="recibo-centro recibo-dato-tienda recibo-ruc">RUC {ruc}</div>}

      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">{encabezado}</div>
      {numeroDocumento && <div className="recibo-centro recibo-comprobante-numero">{numeroDocumento}</div>}
      <div className="recibo-linea"></div>

      {cliente && (
        <>
          <div className="recibo-seccion-titulo">ADQUIRIENTE</div>
          {cliente.numero_documento && (
            <div className="recibo-fila-meta">
              <span>{esFactura ? 'RUC' : 'Doc.'}</span>
              <span>{cliente.numero_documento}</span>
            </div>
          )}
          <div className="recibo-cliente-nombre">{cliente.nombre_razon_social}</div>
          {cliente.direccion && <div className="recibo-cliente-direccion">{cliente.direccion}</div>}
          <div className="recibo-linea"></div>
        </>
      )}

      <div className="recibo-fila-meta">
        <span>Venta</span>
        <span>{venta.folio}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>{esFactura ? 'Emisión' : 'Fecha'}</span>
        <span>{fecha || new Date().toLocaleString('es-PE')}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Moneda</span>
        <span>SOLES</span>
      </div>
      {esFactura && (
        <div className="recibo-fila-meta">
          <span>Forma de pago</span>
          <span>{formaPago}</span>
        </div>
      )}
      <div className="recibo-fila-meta">
        <span>Cajero</span>
        <span>{cajero}</span>
      </div>

      <div className="recibo-linea"></div>

      {items.map((item, idx) => (
        <div key={idx} className="recibo-item">
          <div className="recibo-item-nombre">{item.nombre}</div>
          <div className="recibo-item-detalle">
            <span>{formatoCantidad(item.cantidad)} x S/.{item.precio.toFixed(2)}</span>
            <span>S/.{subtotalLinea(item.precio, item.cantidad).toFixed(2)}</span>
          </div>
        </div>
      ))}

      <div className="recibo-linea"></div>

      <div className="recibo-fila-meta recibo-fila-meta-sutil">
        <span>Ítems</span>
        <span>{items.length} producto{items.length === 1 ? '' : 's'} · {totalUnidades} unid.</span>
      </div>

      {esComprobanteReal && (
        <>
          <div className="recibo-linea"></div>
          <div className="recibo-fila-meta">
            <span>Op. Gravada</span>
            <span>S/.{desglose.gravadas.toFixed(2)}</span>
          </div>
          {desglose.exoneradas > 0 && (
            <div className="recibo-fila-meta">
              <span>Op. Exonerada</span>
              <span>S/.{desglose.exoneradas.toFixed(2)}</span>
            </div>
          )}
          {desglose.inafectas > 0 && (
            <div className="recibo-fila-meta">
              <span>Op. Inafecta</span>
              <span>S/.{desglose.inafectas.toFixed(2)}</span>
            </div>
          )}
          <div className="recibo-fila-meta">
            <span>IGV ({etiquetaTasa(desglose.tasa)}%)</span>
            <span>S/.{igv.toFixed(2)}</span>
          </div>
        </>
      )}

      <div className="recibo-linea-doble"></div>

      <div className="recibo-total">
        <span>TOTAL</span>
        <span>S/.{total.toFixed(2)}</span>
      </div>

      {esComprobanteReal && <div className="recibo-importe-letras">SON: {montoEnLetras(total)}</div>}

      {detraccion && (
        <div className="recibo-detraccion">
          <div className="recibo-detraccion-titulo">OPERACIÓN SUJETA A DETRACCIÓN</div>
          <div className="recibo-fila-meta">
            <span>Detracción ({formatoCantidad(detraccion.detraccion_porcentaje)}%)</span>
            <span>S/.{detraccion.detraccion_monto.toFixed(2)}</span>
          </div>
          {detraccion.detraccion_cuenta && (
            <div className="recibo-fila-meta">
              <span>Cta. Bco. de la Nación</span>
              <span>{detraccion.detraccion_cuenta}</span>
            </div>
          )}
          <div className="recibo-fila-meta">
            <span>Neto a pagar</span>
            <span>S/.{(total - detraccion.detraccion_monto).toFixed(2)}</span>
          </div>
        </div>
      )}

      {venta.metodoPago === 'MIXTO' && venta.pagoOtro != null && (
        <div className="recibo-detalle-pago">
          <span>{nombreMetodo(venta.pagoOtroMetodo)}</span>
          <span>S/.{venta.pagoOtro.toFixed(2)}</span>
        </div>
      )}
      {venta.credito && (
        <div className="recibo-detraccion">
          <div className="recibo-detraccion-titulo">VENTA AL CRÉDITO</div>
          {venta.credito.adelanto > 0 && (
            <div className="recibo-fila-meta">
              <span>Adelanto ({nombreMetodo(venta.credito.adelantoMetodo)})</span>
              <span>S/.{venta.credito.adelanto.toFixed(2)}</span>
            </div>
          )}
          <div className="recibo-fila-meta">
            <span>Saldo por pagar</span>
            <span>S/.{venta.credito.saldo.toFixed(2)}</span>
          </div>
          <div className="recibo-fila-meta">
            <span>Plazo</span>
            <span>{venta.credito.dias} días</span>
          </div>
          {venta.credito.dias > 0 && (
            <div className="recibo-fila-meta">
              <span>Cuota 1 vence</span>
              <span>
                {(() => {
                  const vence = new Date();
                  vence.setDate(vence.getDate() + Number(venta.credito.dias));
                  return vence.toLocaleDateString('es-PE');
                })()}
              </span>
            </div>
          )}
        </div>
      )}
      {cambioPrenda && (
        <div className="recibo-detraccion">
          <div className="recibo-detraccion-titulo">CAMBIO DE PRENDA</div>
          <div className="recibo-fila-meta">
            <span>Venta original</span>
            <span>{cambioPrenda.folioOriginal}</span>
          </div>
          {cambioPrenda.items.map((i) => (
            <div key={i.detalle_id} className="recibo-item">
              <div className="recibo-item-nombre">Devuelve: {i.nombre}</div>
              <div className="recibo-item-detalle">
                <span>{formatoCantidad(i.cantidad)} x S/.{i.valor_unitario.toFixed(2)}</span>
                <span>- S/.{(i.valor_unitario * i.cantidad).toFixed(2)}</span>
              </div>
            </div>
          ))}
          <div className="recibo-fila-meta">
            <span>{cambioPrenda.aDevolver > 0 ? 'Devuelto al cliente' : 'Diferencia pagada'}</span>
            <span>S/.{(cambioPrenda.aDevolver > 0 ? cambioPrenda.aDevolver : cambioPrenda.aCobrar).toFixed(2)}</span>
          </div>
        </div>
      )}
      {venta.montoRecibido != null && !(cambioPrenda && cambioPrenda.aCobrar === 0) && (
        <div className="recibo-detalle-pago">
          <span>Efectivo</span>
          <span>S/.{venta.montoRecibido.toFixed(2)}</span>
        </div>
      )}
      {venta.cambio != null && !(cambioPrenda && cambioPrenda.aCobrar === 0) && (
        <div className="recibo-detalle-pago">
          <span>{cambioPrenda ? 'Vuelto' : 'Cambio'}</span>
          <span>S/.{venta.cambio.toFixed(2)}</span>
        </div>
      )}

      <div className="recibo-linea-doble"></div>

      {qrDataUrl && (
        <div className="recibo-qr-wrapper">
          <img src={qrDataUrl} alt="Código QR SUNAT" className="recibo-qr" />
        </div>
      )}

      {esComprobanteReal && (
        <div className="recibo-centro recibo-disclaimer">
          Representación impresa de la {esFactura ? 'FACTURA ELECTRÓNICA' : 'BOLETA DE VENTA ELECTRÓNICA'}.
        </div>
      )}

      {esComprobanteReal && (
        <div className="recibo-centro recibo-disclaimer">
          Consulte este comprobante en el portal de SUNAT escaneando el
          código QR, o revise el documento oficial disponible en el
          sistema.
        </div>
      )}

      {limiteCambio && (
        <div className="recibo-centro recibo-disclaimer">
          Cambios hasta el {limiteCambio} ({diasCambio} días), con la prenda sin uso y este comprobante.
        </div>
      )}

      <div className="recibo-centro recibo-gracias">¡Gracias por su compra!</div>
    </div>
  );
}