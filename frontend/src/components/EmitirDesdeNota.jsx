import { useEffect, useMemo, useState } from 'react';
import { FileText, X } from 'lucide-react';
import { api } from '../api/api';
import './FormularioGasto.css';
import './EmitirDesdeNota.css';

/**
 * Boleta o factura de una venta que se cobró como nota de venta (el cliente
 * pagó y después pidió su comprobante). La venta no cambia: solo se le emite
 * su comprobante, con fecha de hoy (POST /comprobantes/desde-nota).
 * Para la factura hace falta un cliente con RUC: se busca o se registra con
 * los datos de SUNAT, igual que en el punto de venta.
 */
export default function EmitirDesdeNota({ venta, onHecho, onCerrar }) {
  const [tipo, setTipo] = useState('BOLETA');
  const [cliente, setCliente] = useState(null);
  const [buscar, setBuscar] = useState('');
  const [clientes, setClientes] = useState([]);
  const [consulta, setConsulta] = useState(null);
  const [error, setError] = useState('');
  const [enviando, setEnviando] = useState(false);

  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && !enviando && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar, enviando]);

  useEffect(() => {
    const q = buscar.trim();
    if (q.length < 2) return undefined;
    const t = setTimeout(() => {
      api
        .clientesBuscar(q)
        .then((r) => setClientes((r || []).slice(0, 6)))
        .catch(() => setClientes([]));
    }, 250);
    return () => clearTimeout(t);
  }, [buscar]);

  // DNI (8) o RUC (11) que aún no es cliente: se consulta RENIEC/SUNAT.
  const documento = useMemo(() => {
    const q = buscar.trim();
    if (/^\d{11}$/.test(q)) return { tipo: 'RUC', numero: q };
    if (/^\d{8}$/.test(q)) return { tipo: 'DNI', numero: q };
    return null;
  }, [buscar]);
  const yaEsCliente = !!documento && clientes.some((c) => c.numero_documento === documento.numero);
  useEffect(() => {
    if (!documento || yaEsCliente) return undefined;
    let vivo = true;
    api
      .documentoConsultar(documento.tipo, documento.numero)
      .then((r) => vivo && setConsulta({ numero: documento.numero, nombre: r?.existe ? r.nombre : null, direccion: r?.direccion || null }))
      .catch(() => vivo && setConsulta({ numero: documento.numero, nombre: null }));
    return () => {
      vivo = false;
    };
  }, [documento, yaEsCliente]);
  const encontrado = documento && !yaEsCliente && consulta?.numero === documento.numero ? consulta : null;

  const elegir = (c) => {
    setCliente(c);
    setBuscar('');
    setClientes([]);
    setError('');
  };
  const registrar = async () => {
    setError('');
    try {
      elegir(
        await api.clienteCrear({
          tipo_documento: documento.tipo,
          numero_documento: documento.numero,
          nombre_razon_social: encontrado.nombre,
          direccion: encontrado.direccion || null,
        })
      );
    } catch (e) {
      setError(e.message);
    }
  };

  const ruc = (c) => /^\d{11}$/.test(String(c?.numero_documento || ''));
  const clienteDeVenta = !cliente && venta.cliente_nombre ? venta.cliente_nombre : null;
  const faltaRuc = tipo === 'FACTURA' && !ruc(cliente);

  const emitir = async (e) => {
    e.preventDefault();
    if (enviando) return;
    if (faltaRuc) {
      setError('Para la factura busca o escribe el RUC del cliente.');
      return;
    }
    setEnviando(true);
    setError('');
    try {
      const r = await api.comprobanteDesdeNota({ venta_id: venta.venta_id, tipo, cliente_id: cliente?.id || null });
      onHecho?.(r);
    } catch (err) {
      setError(err.message);
      setEnviando(false);
    }
  };

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && !enviando && onCerrar()}>
      <form className="fg-modal" onSubmit={emitir} role="dialog" aria-modal="true" aria-labelledby="edn-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono">
            <FileText size={18} />
          </span>
          <div>
            <h2 id="edn-titulo">Emitir comprobante de la venta {venta.folio_venta}</h2>
            <p>
              Se cobró como nota de venta (S/ {Number(venta.monto || 0).toFixed(2)}). La venta no cambia: solo sale su boleta o
              factura, con fecha de hoy.
            </p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar" disabled={enviando}>
            <X size={18} />
          </button>
        </header>

        <div className="fg-cuerpo">
          <div className="fg-segmentos" role="radiogroup" aria-label="Tipo de comprobante">
            {[
              ['BOLETA', 'Boleta'],
              ['FACTURA', 'Factura'],
            ].map(([valor, texto]) => (
              <button
                key={valor}
                type="button"
                role="radio"
                aria-checked={tipo === valor}
                className={tipo === valor ? 'activo' : ''}
                onClick={() => {
                  setTipo(valor);
                  setError('');
                }}
              >
                {texto}
              </button>
            ))}
          </div>

          <div className="fg-campo">
            <span>Cliente {tipo === 'FACTURA' ? '(con RUC)' : '(opcional)'}</span>
            {cliente ? (
              <div className="edn-elegido">
                <span>
                  <strong>{cliente.nombre_razon_social}</strong>
                  {cliente.numero_documento && <small> · {cliente.numero_documento}</small>}
                </span>
                <button type="button" className="fg-boton" onClick={() => setCliente(null)}>
                  Cambiar
                </button>
              </div>
            ) : (
              <>
                {clienteDeVenta && <p className="fg-tenue edn-de-venta">De la venta: {clienteDeVenta}. Para otro, búscalo abajo.</p>}
                <input
                  value={buscar}
                  autoFocus={tipo === 'FACTURA'}
                  inputMode={/^\d+$/.test(buscar.trim()) ? 'numeric' : undefined}
                  placeholder={tipo === 'FACTURA' ? 'RUC o razón social' : 'Nombre, DNI o RUC'}
                  onChange={(e) => setBuscar(e.target.value)}
                />
                {clientes.map((c) => (
                  <button key={c.id} type="button" className="edn-fila" onClick={() => elegir(c)}>
                    <strong>{c.nombre_razon_social}</strong>
                    <small>{c.numero_documento || 'Sin documento'}</small>
                  </button>
                ))}
                {documento && !yaEsCliente && !encontrado && <p className="fg-tenue">Consultando {documento.tipo === 'RUC' ? 'SUNAT' : 'RENIEC'}…</p>}
                {encontrado?.nombre && (
                  <div className="edn-elegido">
                    <span>
                      <strong>{encontrado.nombre}</strong>
                      <small>
                        {' '}
                        · {documento.tipo} {documento.numero}
                      </small>
                    </span>
                    <button type="button" className="fg-boton fg-boton-principal" onClick={registrar}>
                      Usar
                    </button>
                  </div>
                )}
                {encontrado && !encontrado.nombre && (
                  <p className="fg-nota fg-nota-error" style={{ margin: 0 }}>
                    No se encontró el {documento.tipo} {documento.numero}. Revísalo o regístralo en Clientes.
                  </p>
                )}
              </>
            )}
          </div>

          {error && <p className="fg-error">{error}</p>}
        </div>

        <footer className="fg-acciones">
          <button type="button" className="fg-boton" onClick={onCerrar} disabled={enviando}>
            Cancelar
          </button>
          <button type="submit" className="fg-boton fg-boton-principal" disabled={enviando || faltaRuc}>
            {enviando ? 'Emitiendo…' : `Emitir ${tipo === 'FACTURA' ? 'factura' : 'boleta'}`}
          </button>
        </footer>
      </form>
    </div>
  );
}
