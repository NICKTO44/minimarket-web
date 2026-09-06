import { useState, useEffect } from 'react';
import QRCode from 'qrcode';
import { api } from '../../api/api';
import { montoEnLetras } from '../../utils/numeroALetras';
import { construirCadenaQrSunat } from '../../utils/qrSunat';
import './BoletaPublica.css';

export default function BoletaPublica({ identificador, comprobanteId }) {
  const [datos, setDatos] = useState(null);
  const [error, setError] = useState(null);
  const [cargando, setCargando] = useState(true);
  const [qrDataUrl, setQrDataUrl] = useState(null);

  useEffect(() => {
    api
      .comprobantePublico(identificador, comprobanteId)
      .then(setDatos)
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  }, [identificador, comprobanteId]);

  useEffect(() => {
    if (!datos?.ruc_emisor || !datos?.hash || !datos?.fecha_emision) {
      setQrDataUrl(null);
      return;
    }

    const cadena = construirCadenaQrSunat({
      ruc: datos.ruc_emisor,
      tipoDocumento: datos.tipo === 'FACTURA' ? '01' : '03',
      serie: datos.serie,
      numero: datos.numero,
      igv: datos.igv,
      total: datos.total,
      fechaEmision: datos.fecha_emision,
      tipoDocCliente: datos.cliente_documento
        ? datos.cliente_documento.length === 11
          ? '6'
          : '1'
        : '0',
      numDocCliente: datos.cliente_documento || '-',
      hash: datos.hash,
    });

    QRCode.toDataURL(cadena, { margin: 0, width: 200 })
      .then(setQrDataUrl)
      .catch(() => setQrDataUrl(null));
  }, [datos]);

  if (cargando) {
    return (
      <div className="bp-layout">
        <p className="bp-cargando">Cargando comprobante...</p>
      </div>
    );
  }

  if (error || !datos) {
    return (
      <div className="bp-layout">
        <div className="bp-error-card">
          <p>No se pudo cargar este comprobante.</p>
          <p className="bp-error-detalle">{error}</p>
        </div>
      </div>
    );
  }

  const numeroDocumento = datos.serie ? `${datos.serie}-${String(datos.numero).padStart(6, '0')}` : null;
  const gravada = datos.total - datos.igv;
  const totalUnidades = datos.items.reduce((sum, item) => sum + item.cantidad, 0);

  return (
    <div className="bp-layout">
      <div className="bp-ticket">
        <div className="bp-centro bp-nombre-tienda">{datos.nombre_tienda}</div>
        {datos.direccion && <div className="bp-centro bp-dato">{datos.direccion}</div>}
        {datos.telefono && <div className="bp-centro bp-dato">Tel: {datos.telefono}</div>}
        {datos.ruc_emisor && <div className="bp-centro bp-dato bp-ruc">RUC {datos.ruc_emisor}</div>}

        <div className="bp-linea"></div>
        <div className="bp-centro bp-tipo-comprobante">
          {datos.tipo === 'FACTURA' ? 'FACTURA ELECTRÓNICA' : 'BOLETA DE VENTA ELECTRÓNICA'}
        </div>
        {numeroDocumento && <div className="bp-centro bp-numero-comprobante">{numeroDocumento}</div>}
        <div className="bp-linea"></div>

        {datos.cliente_nombre && (
          <>
            <div className="bp-seccion-titulo">ADQUIRIENTE</div>
            {datos.cliente_documento && (
              <div className="bp-fila">
                <span>Doc.</span>
                <span>{datos.cliente_documento}</span>
              </div>
            )}
            <div className="bp-cliente-nombre">{datos.cliente_nombre}</div>
            <div className="bp-linea"></div>
          </>
        )}

        <div className="bp-fila">
          <span>Venta</span>
          <span>{datos.folio_venta}</span>
        </div>
        <div className="bp-fila">
          <span>Fecha</span>
          <span>{datos.fecha_emision}</span>
        </div>
        <div className="bp-fila">
          <span>Moneda</span>
          <span>SOLES</span>
        </div>

        <div className="bp-linea"></div>

        {datos.items.map((item, idx) => (
          <div key={idx} className="bp-item">
            <div className="bp-item-nombre">{item.nombre}</div>
            <div className="bp-item-detalle">
              <span>{item.cantidad} x S/.{item.precio.toFixed(2)}</span>
              <span>S/.{(item.precio * item.cantidad).toFixed(2)}</span>
            </div>
          </div>
        ))}

        <div className="bp-linea"></div>

        <div className="bp-fila bp-fila-sutil">
          <span>Ítems</span>
          <span>
            {datos.items.length} producto{datos.items.length === 1 ? '' : 's'} · {totalUnidades} unid.
          </span>
        </div>

        <div className="bp-linea"></div>
        <div className="bp-fila">
          <span>Op. Gravada</span>
          <span>S/.{gravada.toFixed(2)}</span>
        </div>
        <div className="bp-fila">
          <span>IGV (18%)</span>
          <span>S/.{datos.igv.toFixed(2)}</span>
        </div>

        <div className="bp-linea-doble"></div>

        <div className="bp-total">
          <span>TOTAL</span>
          <span>S/.{datos.total.toFixed(2)}</span>
        </div>

        <div className="bp-importe-letras">SON: {montoEnLetras(datos.total)}</div>

        <div className="bp-linea-doble"></div>

        {qrDataUrl && (
          <div className="bp-qr-wrapper">
            <img src={qrDataUrl} alt="Código QR SUNAT" className="bp-qr" />
          </div>
        )}

        <div className="bp-centro bp-disclaimer">
          Representación del comprobante electrónico. Escanea el código QR
          para verificarlo directamente en SUNAT.
        </div>

        <div className="bp-centro bp-gracias">¡Gracias por su compra!</div>
      </div>

      <p className="bp-pie">Generado por Monspeet POS</p>
    </div>
  );
}