import { useEffect, useState } from 'react';
import { api, API_URL } from '../api/api';
import { datosDelTicket, facturaConA4, pagoDeVenta } from '../utils/documentoVenta';
import { datosA4DeDocumento, imprimirComprobanteA4 } from '../utils/facturaA4';
import { descargarArchivo, pdfDeVenta } from '../utils/pdfTicket';
import './VerComprobanteVenta.css';

/**
 * El comprobante de una venta, en SOLO LECTURA (desde Reportes): verlo,
 * descargarlo o imprimirlo. Anular, emitir notas de crédito o reenviar se
 * hace en Comprobantes, no aquí. El documento se arma igual que allí
 * (utils/documentoVenta.js), con los mismos datos del servidor.
 */
export default function VerComprobanteVenta({ venta, onCerrar }) {
  const [comp, setComp] = useState(null);
  const [cfg, setCfg] = useState(null);
  const [error, setError] = useState('');
  const [ocupado, setOcupado] = useState('');

  useEffect(() => {
    let vivo = true;
    Promise.all([api.comprobantesListar({ venta_id: venta.id }), api.configuracionObtener().catch(() => null)])
      .then(([filas, config]) => {
        if (!vivo) return;
        const fila = Array.isArray(filas) ? filas.find((f) => f.venta_id === venta.id) : null;
        if (!fila) setError('No se encontró el comprobante de esta venta.');
        setComp(fila || null);
        setCfg(config);
      })
      .catch((e) => vivo && setError(e.message));
    return () => {
      vivo = false;
    };
  }, [venta.id]);

  useEffect(() => {
    const alPresionar = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alPresionar);
    return () => window.removeEventListener('keydown', alPresionar);
  }, [onCerrar]);

  const esNota = comp?.tipo === 'NINGUNO';
  const titulo = !comp
    ? 'Comprobante de la venta'
    : esNota
      ? `Nota de venta ${venta.folio}`
      : `${comp.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${comp.serie}-${comp.numero}`;

  // El ticket propio en PDF (boleta, factura o nota simple).
  const pdfDelTicket = async () => {
    const datos = await datosDelTicket(comp);
    return pdfDeVenta({
      ...datos,
      cliente:
        datos.cliente ||
        (comp.cliente_nombre ? { nombre_razon_social: comp.cliente_nombre, numero_documento: comp.cliente_documento || null } : null),
      nombreTienda: cfg?.nombre_tienda || '',
      direccion: cfg?.direccion || '',
      telefono: cfg?.telefono || '',
      ruc: cfg?.ruc || comp.ruc_emisor || '',
      // Quien hizo la venta (no quien la está revisando).
      cajero: venta.cajero || '',
    });
  };

  const hacer = async (clave, accion) => {
    setError('');
    setOcupado(clave);
    try {
      await accion();
    } catch (e) {
      setError(e.message || 'No se pudo preparar el documento.');
    } finally {
      setOcupado('');
    }
  };

  // Ver: la pestaña se abre en el momento del clic (si se abre después de
  // esperar, el navegador la bloquea) y se llena cuando el PDF está listo.
  const verTicket = () => {
    const pestana = window.open('', '_blank');
    hacer('ver', async () => {
      try {
        const { bytes } = await pdfDelTicket();
        const url = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }));
        if (pestana) pestana.location.href = url;
        else window.open(url, '_blank');
        setTimeout(() => URL.revokeObjectURL(url), 60000);
      } catch (e) {
        pestana?.close();
        throw e;
      }
    });
  };

  const descargarTicket = () =>
    hacer('descargar', async () => {
      const { bytes, nombre } = await pdfDelTicket();
      descargarArchivo(bytes, nombre);
    });

  // Hoja A4: el PDF oficial de FacturaLibre, o nuestro A4 armado con el
  // documento que se envió a SUNAT (se abre para ver e imprimir).
  const verA4 = () => {
    if (comp.enlace_pdf) {
      window.open(api.comprobantePdfUrl(comp.id), '_blank', 'noopener');
      return;
    }
    hacer('a4', async () => {
      const documento = await api.comprobanteDocumento(comp.id);
      const datos = await datosA4DeDocumento(documento, {
        hash: comp.hash,
        pago: await pagoDeVenta(comp.folio_venta || venta.folio),
        telefono: cfg?.telefono,
        email: cfg?.email,
        logo: cfg?.logo_path ? `${API_URL}${cfg.logo_path}` : null,
        color: cfg?.color_acento,
      });
      imprimirComprobanteA4(datos);
    });
  };

  const conA4 = comp && facturaConA4(comp);

  return (
    <div className="vcv-fondo" onClick={onCerrar}>
      <div className="vcv-caja" role="dialog" aria-modal="true" aria-labelledby="vcv-titulo" onClick={(e) => e.stopPropagation()}>
        <h2 id="vcv-titulo">{titulo}</h2>
        <p className="vcv-sub">
          Venta {venta.folio} · {venta.cajero}
          {comp && !esNota && comp.estado ? ` · ${comp.estado.charAt(0)}${comp.estado.slice(1).toLowerCase()}` : ''}
        </p>

        {!comp && !error && <p className="vcv-sub">Cargando...</p>}

        {comp && (
          <div className="vcv-opciones">
            <button type="button" onClick={verTicket} disabled={!!ocupado}>
              <strong>{ocupado === 'ver' ? 'Preparando...' : 'Ver ticket'}</strong>
              <small>Se abre en una pestaña nueva (PDF)</small>
            </button>
            <button type="button" onClick={descargarTicket} disabled={!!ocupado}>
              <strong>{ocupado === 'descargar' ? 'Preparando...' : 'Descargar ticket'}</strong>
              <small>Guardar el PDF en este equipo</small>
            </button>
            {conA4 && (
              <button type="button" onClick={verA4} disabled={!!ocupado}>
                <strong>{ocupado === 'a4' ? 'Preparando...' : 'Hoja A4'}</strong>
                <small>{comp.enlace_pdf ? 'El PDF oficial de la factura' : 'Con el logo del negocio, para ver o imprimir'}</small>
              </button>
            )}
          </div>
        )}

        {error && <p className="vcv-error">{error}</p>}

        <p className="vcv-nota">
          Solo lectura. Para anular, emitir una nota de crédito o reenviar a SUNAT, usa la sección Comprobantes.
        </p>
        <button type="button" className="vcv-cerrar" onClick={onCerrar}>
          Cerrar
        </button>
      </div>
    </div>
  );
}
