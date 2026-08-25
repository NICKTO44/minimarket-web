export default function Recibo({ venta, items, nombreTienda, direccion, telefono, cajero, comprobante, cliente }) {
  const encabezado = comprobante
    ? `${comprobante.tipo === 'FACTURA' ? 'FACTURA ELECTRÓNICA' : 'BOLETA ELECTRÓNICA'} ${comprobante.serie}-${String(comprobante.numero).padStart(6, '0')}`
    : 'NOTA DE VENTA (sin comprobante tributario)';

  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro">
        <strong>{nombreTienda}</strong>
      </div>
      {direccion && <div className="recibo-centro">{direccion}</div>}
      {telefono && <div className="recibo-centro">Tel: {telefono}</div>}
      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">{encabezado}</div>
      <div>Venta interna: {venta.folio}</div>
      {cliente && (
        <div>
          Cliente: {cliente.nombre_razon_social}
          {cliente.numero_documento ? ` (${cliente.numero_documento})` : ''}
        </div>
      )}
      <div>Cajero: {cajero}</div>
      <div>Fecha: {new Date().toLocaleString('es-PE')}</div>
      <div className="recibo-linea"></div>

      {items.map((item, idx) => (
        <div key={idx} className="recibo-item">
          <div>{item.nombre}</div>
          <div className="recibo-item-detalle">
            <span>{item.cantidad} x S/.{item.precio.toFixed(2)}</span>
            <span>S/.{(item.precio * item.cantidad).toFixed(2)}</span>
          </div>
        </div>
      ))}

      <div className="recibo-linea"></div>
      <div className="recibo-total">
        <span>TOTAL:</span>
        <span>S/.{venta.total.toFixed(2)}</span>
      </div>
      {venta.montoRecibido != null && (
        <div className="recibo-detalle-pago">
          <span>Efectivo:</span>
          <span>S/.{venta.montoRecibido.toFixed(2)}</span>
        </div>
      )}
      {venta.cambio != null && (
        <div className="recibo-detalle-pago">
          <span>Cambio:</span>
          <span>S/.{venta.cambio.toFixed(2)}</span>
        </div>
      )}
      <div className="recibo-linea"></div>
      <div className="recibo-centro">¡Gracias por su compra!</div>
    </div>
  );
}