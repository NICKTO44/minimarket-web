import { useState, useEffect } from 'react';
import QRCode from 'qrcode';
import { montoEnLetras } from '../utils/numeroALetras';
import { construirCadenaQrSunat } from '../utils/qrSunat';

export default function Recibo({ venta, items, nombreTienda, direccion, telefono, ruc, cajero, comprobante, cliente }) {
  const esComprobanteReal = !!comprobante;
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
  const igv = comprobante?.igv ?? (total - total / 1.18);
  const gravada = total - igv;

  const totalUnidades = items.reduce((sum, item) => sum + item.cantidad, 0);

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

    QRCode.toDataURL(cadena, { margin: 0, width: 160 })
      .then(setQrDataUrl)
      .catch(() => setQrDataUrl(null));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [puedeGenerarQr, comprobante?.hash, comprobante?.serie, comprobante?.numero]);

  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
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
              <span>Doc.</span>
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
        <span>Fecha</span>
        <span>{new Date().toLocaleString('es-PE')}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Moneda</span>
        <span>SOLES</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Cajero</span>
        <span>{cajero}</span>
      </div>

      <div className="recibo-linea"></div>

      {items.map((item, idx) => (
        <div key={idx} className="recibo-item">
          <div className="recibo-item-nombre">{item.nombre}</div>
          <div className="recibo-item-detalle">
            <span>{item.cantidad} x S/.{item.precio.toFixed(2)}</span>
            <span>S/.{(item.precio * item.cantidad).toFixed(2)}</span>
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
            <span>S/.{gravada.toFixed(2)}</span>
          </div>
          <div className="recibo-fila-meta">
            <span>IGV (18%)</span>
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

      {venta.montoRecibido != null && (
        <div className="recibo-detalle-pago">
          <span>Efectivo</span>
          <span>S/.{venta.montoRecibido.toFixed(2)}</span>
        </div>
      )}
      {venta.cambio != null && (
        <div className="recibo-detalle-pago">
          <span>Cambio</span>
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
          Consulte este comprobante en el portal de SUNAT escaneando el
          código QR, o revise el documento oficial disponible en el
          sistema.
        </div>
      )}

      <div className="recibo-centro recibo-gracias">¡Gracias por su compra!</div>
    </div>
  );
}