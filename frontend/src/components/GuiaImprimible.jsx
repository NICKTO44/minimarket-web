import { useEffect, useState } from 'react';
import QRCode from 'qrcode';
import './Recibo.css';
import { fechaCorta } from '../utils/formato';
import { formatoCantidad } from '../utils/medidas';
import { abreviaturaUnidad } from '../utils/unidades';

const NOMBRE_DOCUMENTO = { RUC: 'RUC', DNI: 'DNI', CE: 'C.E.', PASAPORTE: 'Pasaporte' };

/** "T001-75" -> "T001-000075", como el resto de tickets. */
function numeroConCeros(numero) {
  const [serie, correlativo] = String(numero || '').split('-');
  return correlativo ? `${serie}-${correlativo.padStart(6, '0')}` : serie;
}

/**
 * Guía de remisión para la ticketera de 80 mm (mismo formato angosto que el
 * ticket de venta). Solo existe al imprimir: en pantalla está oculta. No
 * lleva precios: la guía sustenta el traslado, no la venta.
 *
 * `guia.datos` es el formulario con que se emitió (lo entrega el servidor al
 * abrir la guía). `guia.qr` es el QR de SUNAT, si FacturaLibre lo entregó:
 * un enlace (se dibuja aquí) o la imagen ya hecha. Sin él, el ticket sale
 * sin QR; no se inventa uno.
 */
export default function GuiaImprimible({ guia, motivos, nombreTienda, direccion, telefono, ruc }) {
  const datos = guia?.datos || null;
  const qr = guia?.estado === 'ACEPTADA' ? guia?.qr || null : null;
  const [ubigeos, setUbigeos] = useState(null);
  const [qrDibujado, setQrDibujado] = useState({ de: null, imagen: null });

  // Los distritos (para escribir "Wanchaq - Cusco - Cusco" junto a la dirección).
  const hayGuia = !!datos;
  useEffect(() => {
    if (!hayGuia) return undefined;
    let vigente = true;
    import('../data/ubigeos')
      .then((m) => vigente && setUbigeos(m.UBIGEOS))
      .catch(() => {});
    return () => {
      vigente = false;
    };
  }, [hayGuia]);

  useEffect(() => {
    if (!qr || qr.startsWith('data:image/')) return undefined;
    let vigente = true;
    // `scale`: cada cuadro mide píxeles enteros y sale nítido en la ticketera.
    QRCode.toDataURL(qr, { margin: 0, scale: 6 })
      .then((imagen) => vigente && setQrDibujado({ de: qr, imagen }))
      .catch(() => {});
    return () => {
      vigente = false;
    };
  }, [qr]);

  if (!datos) return null;

  const imagenQr = !qr ? null : qr.startsWith('data:image/') ? qr : qrDibujado.de === qr ? qrDibujado.imagen : null;
  const lugar = (punto) => {
    const u = ubigeos?.find((x) => x.ubigeo === punto?.ubigeo);
    const direccionPunto = punto?.direccion || '';
    return u ? `${direccionPunto}, ${u.distrito} - ${u.provincia} - ${u.departamento}` : direccionPunto;
  };
  const motivo =
    (datos.motivo === '13' && datos.motivo_descripcion) ||
    motivos?.find((m) => m.codigo === datos.motivo)?.nombre ||
    datos.motivo_descripcion ||
    '';
  const privado = datos.modo !== 'PUBLICO';
  const items = datos.items || [];

  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
      {ruc && <div className="recibo-centro recibo-dato-tienda recibo-ruc">RUC {ruc}</div>}

      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">
        GUÍA DE REMISIÓN ELECTRÓNICA
        <br />
        REMITENTE
      </div>
      <div className="recibo-centro recibo-comprobante-numero">{numeroConCeros(guia.numero)}</div>
      <div className="recibo-linea"></div>

      <div className="recibo-seccion-titulo">DESTINATARIO</div>
      <div className="recibo-fila-meta">
        <span>{NOMBRE_DOCUMENTO[datos.destinatario_tipo] || 'Doc.'}</span>
        <span>{datos.destinatario_documento}</span>
      </div>
      <div className="recibo-cliente-nombre">{datos.destinatario_nombre}</div>

      <div className="recibo-linea"></div>
      <div className="recibo-fila-meta">
        <span>Emisión</span>
        <span>{fechaCorta(guia.fecha)}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Inicio de traslado</span>
        <span>{fechaCorta(datos.fecha_traslado)}</span>
      </div>
      {motivo && (
        <div className="recibo-fila-meta">
          <span>Motivo</span>
          <span>{motivo}</span>
        </div>
      )}
      <div className="recibo-fila-meta">
        <span>Modalidad</span>
        <span>{privado ? 'Transporte privado' : 'Transporte público'}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Peso bruto total</span>
        <span>{formatoCantidad(datos.peso_total)} kg</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Bultos</span>
        <span>{datos.bultos}</span>
      </div>
      {guia.comprobante ? (
        <div className="recibo-fila-meta">
          <span>Comprobante</span>
          <span>{numeroConCeros(guia.comprobante)}</span>
        </div>
      ) : (
        guia.folio_venta && (
          <div className="recibo-fila-meta">
            <span>Venta</span>
            <span>{guia.folio_venta}</span>
          </div>
        )
      )}

      <div className="recibo-linea"></div>
      <div className="recibo-seccion-titulo">PUNTO DE PARTIDA</div>
      <div className="recibo-cliente-direccion">{lugar(datos.partida)}</div>
      <div className="recibo-seccion-titulo" style={{ marginTop: 4 }}>
        PUNTO DE LLEGADA
      </div>
      <div className="recibo-cliente-direccion">{lugar(datos.llegada)}</div>

      <div className="recibo-linea"></div>
      <div className="recibo-seccion-titulo">TRANSPORTE</div>
      {privado ? (
        <>
          {datos.placa && (
            <div className="recibo-fila-meta">
              <span>Placa</span>
              <span>{datos.placa}</span>
            </div>
          )}
          {datos.chofer && (
            <>
              <div className="recibo-fila-meta">
                <span>Conductor</span>
                <span>
                  {datos.chofer.nombres} {datos.chofer.apellidos}
                </span>
              </div>
              <div className="recibo-fila-meta">
                <span>DNI</span>
                <span>{datos.chofer.documento}</span>
              </div>
              <div className="recibo-fila-meta">
                <span>Licencia</span>
                <span>{datos.chofer.licencia}</span>
              </div>
            </>
          )}
        </>
      ) : (
        datos.transportista && (
          <>
            <div className="recibo-cliente-nombre">{datos.transportista.nombre}</div>
            <div className="recibo-fila-meta">
              <span>RUC</span>
              <span>{datos.transportista.ruc}</span>
            </div>
            {datos.transportista.mtc && (
              <div className="recibo-fila-meta">
                <span>Registro MTC</span>
                <span>{datos.transportista.mtc}</span>
              </div>
            )}
          </>
        )
      )}

      <div className="recibo-linea"></div>
      <div className="recibo-seccion-titulo" style={{ marginBottom: 3 }}>
        BIENES A TRASLADAR
      </div>
      {items.map((item, i) => (
        <div key={i} className="recibo-item">
          <div className="recibo-item-nombre">{item.descripcion}</div>
          <div className="recibo-item-detalle">
            <span>{item.codigo}</span>
            <span>
              {formatoCantidad(item.cantidad)} {abreviaturaUnidad(item.unidad)}
            </span>
          </div>
        </div>
      ))}
      <div className="recibo-linea"></div>
      <div className="recibo-fila-meta">
        <span>Ítems</span>
        <span>{items.length}</span>
      </div>
      {datos.observaciones && <div className="recibo-importe-letras">Obs.: {datos.observaciones}</div>}

      <div className="recibo-linea-doble"></div>
      {imagenQr && (
        <div className="recibo-qr-wrapper">
          <img src={imagenQr} alt="Código QR de SUNAT" className="recibo-qr" />
        </div>
      )}
      <div className="recibo-centro recibo-disclaimer">Representación impresa de la GUÍA DE REMISIÓN ELECTRÓNICA REMITENTE.</div>
      <div className="recibo-centro recibo-disclaimer">
        {imagenQr
          ? 'Consulte este documento en SUNAT escaneando el código QR.'
          : 'Consulte este documento en SUNAT con el RUC del remitente y el número de la guía.'}
      </div>
    </div>
  );
}
