import './Recibo.css';
import { formatoCantidad } from '../utils/medidas';
import { abreviaturaUnidad } from '../utils/unidades';
import { fechaCorta, numeroCotizacion } from '../utils/formato';

/**
 * Cotización para imprimir en la ticketera (mismo formato angosto que el
 * ticket de venta). Solo existe al imprimir: en pantalla está oculta. No es
 * un comprobante de pago y lo dice.
 */
export default function CotizacionImprimible({ cotizacion, nombreTienda, direccion, telefono, ruc }) {
  if (!cotizacion) return null;
  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
      {ruc && <div className="recibo-centro recibo-dato-tienda recibo-ruc">RUC {ruc}</div>}

      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">COTIZACIÓN</div>
      <div className="recibo-centro recibo-comprobante-numero">N° {numeroCotizacion(cotizacion.numero)}</div>
      <div className="recibo-linea"></div>

      {cotizacion.cliente_nombre && (
        <>
          <div className="recibo-seccion-titulo">CLIENTE</div>
          {cotizacion.cliente_documento && (
            <div className="recibo-fila-meta">
              <span>Doc.</span>
              <span>{cotizacion.cliente_documento}</span>
            </div>
          )}
          <div className="recibo-cliente-nombre">{cotizacion.cliente_nombre}</div>
          <div className="recibo-linea"></div>
        </>
      )}

      <div className="recibo-fila-meta">
        <span>Fecha</span>
        <span>{fechaCorta(cotizacion.fecha)}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Válida hasta</span>
        <span>{fechaCorta(cotizacion.vence)}</span>
      </div>
      {cotizacion.usuario && (
        <div className="recibo-fila-meta">
          <span>Atendió</span>
          <span>{cotizacion.usuario}</span>
        </div>
      )}

      <div className="recibo-linea"></div>

      {cotizacion.items.map((item, idx) => (
        <div key={idx} className="recibo-item">
          <div className="recibo-item-nombre">{item.detalle ? `${item.nombre} (${item.detalle})` : item.nombre}</div>
          <div className="recibo-item-detalle">
            <span>
              {formatoCantidad(item.cantidad)} {abreviaturaUnidad(item.unidad_medida)} x S/.{item.precio_unitario.toFixed(2)}
            </span>
            <span>S/.{item.total_linea.toFixed(2)}</span>
          </div>
        </div>
      ))}

      <div className="recibo-linea-doble"></div>
      <div className="recibo-total">
        <span>TOTAL</span>
        <span>S/.{cotizacion.total.toFixed(2)}</span>
      </div>
      <div className="recibo-centro recibo-disclaimer">Precios con IGV incluido.</div>

      {cotizacion.notas && <div className="recibo-importe-letras">Nota: {cotizacion.notas}</div>}

      <div className="recibo-linea-doble"></div>
      <div className="recibo-centro recibo-disclaimer">
        Esta cotización no es un comprobante de pago. Precios válidos hasta el {fechaCorta(cotizacion.vence)} y
        sujetos a stock.
      </div>
      <div className="recibo-centro recibo-gracias">¡Gracias por su preferencia!</div>
    </div>
  );
}
