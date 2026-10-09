import { useEffect, useState } from 'react';
import { AlertTriangle, CheckCircle2, Circle, FileKey2, PlugZap, Send, XCircle } from 'lucide-react';
import { panelApi } from '../../api/panel';
import { confirmar } from '../../utils/confirmar';
import SelectorUbigeo from '../../components/SelectorUbigeo';
import { fechaCorta } from './formato';

const sinTildes = (t) =>
  String(t || '')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .trim()
    .toUpperCase();

const CAMPOS_TEXTO = [
  ['ruc', 'RUC', 'Los 11 dígitos', 11],
  ['razon_social', 'Razón social', 'Exacta, como figura en SUNAT'],
  ['nombre_tienda', 'Nombre comercial', 'El que ve el cliente'],
  ['direccion', 'Dirección fiscal', 'Como figura en la ficha RUC'],
];

/**
 * Facturación electrónica de un negocio: cómo emite (FacturaLibre o directo
 * a SUNAT) y, para emitir directo, sus datos de emisor, su certificado y su
 * usuario secundario SOL. Lo que se guarda aquí es lo mismo que hacía el
 * comando `sunat`.
 */
export default function PanelFacturacion({ negocio, onCambio }) {
  const [detalle, setDetalle] = useState(null);
  const [error, setError] = useState('');

  const cargar = async () => {
    try {
      setDetalle(await panelApi.facturacion(negocio.id));
      setError('');
    } catch (e) {
      setError(e.message);
    }
  };

  useEffect(() => {
    let vivo = true;
    panelApi
      .facturacion(negocio.id)
      .then((d) => vivo && setDetalle(d))
      .catch((e) => vivo && setError(e.message));
    return () => {
      vivo = false;
    };
  }, [negocio.id]);

  const alCambiar = async () => {
    await cargar();
    await onCambio();
  };

  if (error && !detalle) return <p className="pnl-error">{error}</p>;
  if (!detalle) return <p className="pnl-tenue">Leyendo la configuración del negocio...</p>;

  const datosCompletos = !detalle.falta;
  const dadoDeAlta = !!detalle.alta?.fecha;

  return (
    <div className="pnl-pasos">
      {detalle.migracion_pendiente && (
        <p className="pnl-aviso pnl-aviso-error">
          Este negocio todavía no tiene la actualización 0021. Detén y vuelve a iniciar el backend (cargo run) para que se aplique.
        </p>
      )}
      {!detalle.lycet_configurado && (
        <p className="pnl-aviso pnl-aviso-error">
          El backend no sabe dónde está Lycet: agrega LYCET_URL y LYCET_TOKEN en backend/.env y reinicia.
        </p>
      )}

      <Modo detalle={detalle} negocio={negocio} listo={datosCompletos} dadoDeAlta={dadoDeAlta} onCambio={alCambiar} />

      <Paso numero={1} titulo="Datos del emisor" hecho={datosCompletos} pendiente={detalle.falta}>
        <DatosEmisor negocio={negocio} datos={detalle.datos} onGuardado={alCambiar} />
      </Paso>

      <Paso
        numero={2}
        titulo="Certificado y usuario SOL"
        hecho={dadoDeAlta}
        pendiente={dadoDeAlta ? null : detalle.modo === 'DIRECTO' ? 'Dado de alta con el comando: vuelve a registrarlo aquí para ver sus datos.' : 'Aún no registrado en Lycet.'}
      >
        <Alta negocio={negocio} detalle={detalle} habilitado={datosCompletos && detalle.lycet_configurado} onHecho={alCambiar} />
      </Paso>

      <Paso numero={3} titulo="Probar conexión" hecho={false} sinEstado>
        <Probar negocio={negocio} />
      </Paso>
    </div>
  );
}

function Paso({ numero, titulo, hecho, pendiente, sinEstado, children }) {
  return (
    <div className="pnl-caja pnl-paso">
      <div className="pnl-paso-cabecera">
        <span className={`pnl-paso-numero${hecho ? ' hecho' : ''}`}>{hecho ? <CheckCircle2 size={18} /> : numero}</span>
        <div>
          <h2>{titulo}</h2>
          {!sinEstado && <small className={hecho ? 'pnl-texto-verde' : 'pnl-texto-ambar'}>{hecho ? 'Listo' : pendiente}</small>}
        </div>
      </div>
      {children}
    </div>
  );
}

function Modo({ detalle, negocio, listo, dadoDeAlta, onCambio }) {
  const [ocupado, setOcupado] = useState(false);
  const [aviso, setAviso] = useState('');
  const directo = detalle.modo === 'DIRECTO';
  const emitidos = Object.entries(detalle.emitidos || {});

  const cambiar = async (modo) => {
    if ((modo === 'DIRECTO') === directo) return;
    const aDirecto = modo === 'DIRECTO';
    const ok = await confirmar({
      titulo: aDirecto ? '¿Emitir directo a SUNAT?' : '¿Volver a FacturaLibre?',
      mensaje: aDirecto
        ? dadoDeAlta
          ? `Desde ahora sus boletas y facturas salen por tu Lycet (${detalle.alta.ambiente === 'BETA' ? 'beta, sin valor legal' : 'producción'}).`
          : 'Ojo: no aparece un alta hecha desde el panel. Si nunca lo registraste en Lycet, sus comprobantes se firmarán con otro certificado y SUNAT los rechazará.'
        : detalle.facturalibre_configurado
          ? 'Sus comprobantes vuelven a salir por FacturaLibre, con su token de siempre.'
          : 'Este negocio no tiene token de FacturaLibre: quedará sin poder emitir boletas ni facturas.',
      detalle: { etiqueta: 'Negocio', valor: negocio.nombre },
      textoConfirmar: aDirecto ? 'Emitir directo' : 'Usar FacturaLibre',
      tipo: aDirecto && dadoDeAlta ? 'normal' : 'peligro',
      icono: 'aviso',
    });
    if (!ok) return;
    setOcupado(true);
    setAviso('');
    try {
      await panelApi.cambiarModo(negocio.id, modo);
      await onCambio();
    } catch (e) {
      setAviso(e.message);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="pnl-caja pnl-modo">
      <div>
        <h2>¿Cómo emite este negocio?</h2>
        <p className="pnl-nota">
          {directo
            ? `Directo a SUNAT por tu Lycet${detalle.alta?.ambiente === 'BETA' ? ' · ambiente BETA (pruebas, sin valor legal)' : detalle.alta?.ambiente === 'PRODUCCION' ? ' · producción' : ''}.`
            : detalle.modo === 'FACTURALIBRE'
              ? 'Por FacturaLibre (con su token y ruta).'
              : 'Todavía no puede emitir boletas ni facturas.'}
        </p>
        {emitidos.length > 0 && (
          <p className="pnl-nota">
            Emitidos directo: {emitidos.map(([estado, n]) => `${n} ${estado.toLowerCase()}`).join(' · ')}
          </p>
        )}
        {aviso && <p className="pnl-error">{aviso}</p>}
      </div>
      <div className="pnl-segmentos" role="radiogroup" aria-label="Modo de emisión">
        <button type="button" role="radio" aria-checked={!directo} className={!directo ? 'activo' : ''} disabled={ocupado} onClick={() => cambiar('FACTURALIBRE')}>
          FacturaLibre
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={directo}
          className={directo ? 'activo pnl-seg-activo' : ''}
          disabled={ocupado || (!directo && !listo)}
          title={!listo ? 'Primero completa los datos del emisor' : undefined}
          onClick={() => cambiar('DIRECTO')}
        >
          Directo a SUNAT
        </button>
      </div>
    </div>
  );
}

function DatosEmisor({ negocio, datos, onGuardado }) {
  const [form, setForm] = useState(datos);
  const [estado, setEstado] = useState(null); // { tipo, texto }
  const [guardando, setGuardando] = useState(false);

  const [ubigeos, setUbigeos] = useState(null);

  useEffect(() => {
    let vivo = true;
    import('../../data/ubigeos')
      .then((m) => vivo && setUbigeos(m.UBIGEOS))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, []);

  const cambiados = Object.keys(form).filter((k) => (form[k] || '').trim() !== (datos[k] || '').trim());
  const poner = (campo, valor) => setForm((f) => ({ ...f, [campo]: valor }));

  // ¿El distrito escrito corresponde al ubigeo? (SUNAT usa los dos.)
  const delUbigeo = ubigeos && form.ubigeo ? ubigeos.find((u) => u.ubigeo === form.ubigeo) : null;
  const distritoNoCoincide = delUbigeo && sinTildes(delUbigeo.distrito) !== sinTildes(form.distrito);

  // Al elegir el distrito se completan departamento, provincia y distrito.
  const elegirUbigeo = async (codigo) => {
    poner('ubigeo', codigo);
    if (!codigo) return;
    try {
      const lista = ubigeos || (await import('../../data/ubigeos')).UBIGEOS;
      const u = lista.find((x) => x.ubigeo === codigo);
      if (u) {
        setForm((f) => ({
          ...f,
          ubigeo: codigo,
          departamento: u.departamento.toUpperCase(),
          provincia: u.provincia.toUpperCase(),
          distrito: u.distrito.toUpperCase(),
        }));
      }
    } catch {
      // sin la lista: se escriben a mano
    }
  };

  const guardar = async (e) => {
    e.preventDefault();
    setGuardando(true);
    setEstado(null);
    try {
      const cambios = Object.fromEntries(cambiados.map((k) => [k, form[k] || '']));
      await panelApi.guardarDatos(negocio.id, cambios);
      setEstado({ tipo: 'ok', texto: 'Datos guardados.' });
      await onGuardado();
    } catch (err) {
      setEstado({ tipo: 'error', texto: err.message });
    } finally {
      setGuardando(false);
    }
  };

  return (
    <form className="pnl-formulario" onSubmit={guardar}>
      {CAMPOS_TEXTO.map(([campo, etiqueta, ayuda, largo]) => (
        <label key={campo} className={`pnl-campo${campo === 'direccion' || campo === 'razon_social' ? ' pnl-ancho' : ''}`}>
          <span>{etiqueta}</span>
          <input
            value={form[campo] || ''}
            maxLength={largo}
            inputMode={campo === 'ruc' ? 'numeric' : undefined}
            placeholder={ayuda}
            onChange={(e) => poner(campo, campo === 'ruc' ? e.target.value.replace(/\D/g, '') : e.target.value)}
          />
        </label>
      ))}
      <div className="pnl-campo pnl-ancho">
        <span>Distrito del domicilio fiscal (ubigeo)</span>
        <SelectorUbigeo valor={form.ubigeo || ''} onCambiar={elegirUbigeo} etiqueta="Distrito fiscal" />
        {form.ubigeo && !distritoNoCoincide && (
          <small className="pnl-nota">Ubigeo {form.ubigeo}: departamento, provincia y distrito se completan solos.</small>
        )}
        {distritoNoCoincide && (
          <small className="pnl-texto-ambar">
            El ubigeo {form.ubigeo} es {delUbigeo.distrito}, pero el distrito dice "{form.distrito || 'vacío'}". Elige de nuevo el distrito o
            corrígelo abajo.
          </small>
        )}
      </div>
      {form.ubigeo &&
        [
          ['departamento', 'Departamento'],
          ['provincia', 'Provincia'],
          ['distrito', 'Distrito'],
        ].map(([campo, etiqueta]) => (
          <label key={campo} className="pnl-campo pnl-tercio">
            <span>{etiqueta}</span>
            <input value={form[campo] || ''} onChange={(e) => poner(campo, e.target.value.toUpperCase())} />
          </label>
        ))}
      <label className="pnl-campo">
        <span>Serie de boletas</span>
        <input value={form.serie_boleta || ''} maxLength={4} placeholder="BM01" onChange={(e) => poner('serie_boleta', e.target.value.toUpperCase())} />
      </label>
      <label className="pnl-campo">
        <span>Serie de facturas</span>
        <input value={form.serie_factura || ''} maxLength={4} placeholder="FM01" onChange={(e) => poner('serie_factura', e.target.value.toUpperCase())} />
      </label>
      <label className="pnl-campo">
        <span>Notas de crédito de boletas</span>
        <input value={form.serie_nc_boleta || ''} maxLength={4} placeholder="BC01" onChange={(e) => poner('serie_nc_boleta', e.target.value.toUpperCase())} />
      </label>
      <label className="pnl-campo">
        <span>Notas de crédito de facturas</span>
        <input value={form.serie_nc_factura || ''} maxLength={4} placeholder="FC01" onChange={(e) => poner('serie_nc_factura', e.target.value.toUpperCase())} />
      </label>
      <div className="pnl-acciones pnl-ancho">
        {estado && <span className={estado.tipo === 'ok' ? 'pnl-texto-verde' : 'pnl-error'}>{estado.texto}</span>}
        <button type="submit" className="pnl-boton pnl-boton-principal" disabled={guardando || cambiados.length === 0}>
          {guardando ? 'Guardando...' : cambiados.length ? `Guardar (${cambiados.length})` : 'Sin cambios'}
        </button>
      </div>
    </form>
  );
}

function Alta({ negocio, detalle, habilitado, onHecho }) {
  const alta = detalle.alta || {};
  const [abierto, setAbierto] = useState(!alta.fecha);
  const [archivo, setArchivo] = useState(null);
  const [claveCertificado, setClaveCertificado] = useState('');
  const [usuarioSol, setUsuarioSol] = useState(alta.usuario_sol || '');
  const [claveSol, setClaveSol] = useState('');
  const [ambiente, setAmbiente] = useState(alta.ambiente === 'PRODUCCION' ? 'produccion' : 'beta');
  // Credenciales API de SUNAT para guías (opcionales; van juntas).
  // Empiezan vacías: Lycet guarda el alta completa, así que al registrar de
  // nuevo hay que volver a escribirlas (si no, se quitan).
  const [greId, setGreId] = useState('');
  const [greClave, setGreClave] = useState('');
  const [enviando, setEnviando] = useState(false);
  const [resultado, setResultado] = useState(null); // { tipo, texto, aviso }

  const esPem = archivo && /\.pem$/i.test(archivo.name);
  const greIncompleto = !!greId.trim() !== !!greClave.trim();
  const completo = archivo && (esPem || claveCertificado) && usuarioSol.trim() && claveSol.trim() && !greIncompleto;

  const registrar = async (e) => {
    e.preventDefault();
    if (ambiente === 'produccion') {
      const ok = await confirmar({
        titulo: '¿Registrar en PRODUCCIÓN?',
        mensaje: 'Cada boleta y factura que emita este negocio será real ante SUNAT.',
        detalle: { etiqueta: 'RUC', valor: detalle.datos.ruc },
        textoConfirmar: 'Sí, producción',
        icono: 'aviso',
      });
      if (!ok) return;
    }
    setEnviando(true);
    setResultado(null);
    try {
      const r = await panelApi.darDeAlta(negocio.id, { archivo, claveCertificado, usuarioSol, claveSol, ambiente, greId, greClave });
      setResultado({
        tipo: 'ok',
        texto: `Registrado en Lycet (${r.ambiente === 'BETA' ? 'beta' : 'producción'}). Certificado de "${r.cert_titular}", vigente hasta ${fechaCorta(r.cert_vence)}. Ya emite directo.`,
        aviso: r.aviso,
      });
      setClaveSol('');
      setGreClave('');
      setClaveCertificado('');
      setArchivo(null);
      setAbierto(false);
      await onHecho();
    } catch (err) {
      setResultado({ tipo: 'error', texto: err.message });
    } finally {
      setEnviando(false);
    }
  };

  return (
    <div>
      {alta.fecha && (
        <dl className="pnl-datos">
          <div>
            <dt>Usuario SOL</dt>
            <dd>
              <small>{detalle.datos.ruc} +</small>
              {alta.usuario_sol}
            </dd>
          </div>
          <div>
            <dt>Ambiente</dt>
            <dd>{alta.ambiente === 'BETA' ? 'Beta (pruebas)' : 'Producción'}</dd>
          </div>
          <div>
            <dt>Certificado</dt>
            <dd>{alta.cert_titular}</dd>
          </div>
          <div>
            <dt>Vence</dt>
            <dd>{fechaCorta(alta.cert_vence)}</dd>
          </div>
          <div>
            <dt>Registrado</dt>
            <dd>{fechaCorta(alta.fecha)}</dd>
          </div>
          <div>
            <dt>Guías de remisión</dt>
            <dd>{alta.gre_client_id ? 'Con credenciales API' : 'Sin credenciales API'}</dd>
          </div>
        </dl>
      )}

      {resultado && (
        <div className={`pnl-aviso pnl-aviso-${resultado.tipo}`}>
          <p>{resultado.texto}</p>
          {resultado.aviso && (
            <p className="pnl-texto-ambar">
              <AlertTriangle size={14} /> {resultado.aviso}
            </p>
          )}
        </div>
      )}

      {!abierto && (
        <button type="button" className="pnl-boton" onClick={() => setAbierto(true)}>
          <FileKey2 size={15} /> {alta.fecha ? 'Cambiar certificado, usuario o ambiente' : 'Registrar'}
        </button>
      )}

      {abierto && (
        <form className="pnl-formulario" onSubmit={registrar}>
          <p className="pnl-nota pnl-ancho">
            El cliente saca en SUNAT su certificado (CDT, archivo .p12 con una clave) y crea un usuario secundario SOL con permiso de
            emisión electrónica. La clave SOL y el certificado se guardan solo en Lycet, no en Monspeet.
          </p>
          <label className="pnl-campo pnl-ancho">
            <span>Certificado (.p12, .pfx o .pem)</span>
            <input type="file" accept=".p12,.pfx,.pem" onChange={(e) => setArchivo(e.target.files?.[0] || null)} />
          </label>
          {!esPem && (
            <label className="pnl-campo">
              <span>Clave del certificado</span>
              <input type="password" value={claveCertificado} onChange={(e) => setClaveCertificado(e.target.value)} autoComplete="off" />
            </label>
          )}
          <label className="pnl-campo">
            <span>Usuario secundario SOL</span>
            <div className="pnl-prefijo">
              <em>{detalle.datos.ruc || 'RUC'}</em>
              <input
                value={usuarioSol}
                maxLength={8}
                placeholder="MONSPEET"
                onChange={(e) => setUsuarioSol(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ''))}
                autoComplete="off"
              />
            </div>
          </label>
          <label className="pnl-campo">
            <span>Clave SOL de ese usuario</span>
            <input type="password" value={claveSol} onChange={(e) => setClaveSol(e.target.value)} autoComplete="new-password" />
          </label>
          <div className="pnl-campo">
            <span>Ambiente</span>
            <div className="pnl-segmentos">
              <button type="button" className={ambiente === 'beta' ? 'activo' : ''} onClick={() => setAmbiente('beta')}>
                Beta (pruebas)
              </button>
              <button type="button" className={ambiente === 'produccion' ? 'activo pnl-seg-peligro' : ''} onClick={() => setAmbiente('produccion')}>
                Producción
              </button>
            </div>
          </div>
          {ambiente === 'beta' && (
            <p className="pnl-nota pnl-ancho">
              En beta SUNAT acepta usuario <strong>MODDATOS</strong> con clave <strong>moddatos</strong>; nada tiene valor legal.
            </p>
          )}
          <p className="pnl-nota pnl-ancho">
            <strong>Guías de remisión (opcional).</strong> SUNAT las recibe por otra vía que pide credenciales API: el cliente las
            genera en SUNAT Operaciones en Línea (Empresas → Credenciales de API SUNAT) para el mismo usuario secundario. Si las
            agregas después, vuelve a registrar con el certificado.
          </p>
          <label className="pnl-campo">
            <span>ID de credenciales API (client_id)</span>
            <input value={greId} onChange={(e) => setGreId(e.target.value.trim())} autoComplete="off" />
          </label>
          <label className="pnl-campo">
            <span>Clave de credenciales API (client_secret)</span>
            <input type="password" value={greClave} onChange={(e) => setGreClave(e.target.value)} autoComplete="new-password" />
          </label>
          {greIncompleto && <span className="pnl-texto-ambar pnl-ancho">Para las guías hacen falta el ID y la clave.</span>}
          {alta.gre_client_id && !greId.trim() && (
            <span className="pnl-texto-ambar pnl-ancho">
              <AlertTriangle size={14} /> Este negocio tiene credenciales de guías. Si no las escribes de nuevo, se quitan al registrar.
            </span>
          )}
          <div className="pnl-acciones pnl-ancho">
            {!habilitado && <span className="pnl-texto-ambar">Primero completa los datos del emisor.</span>}
            {alta.fecha && (
              <button type="button" className="pnl-boton pnl-boton-texto" onClick={() => setAbierto(false)}>
                Cancelar
              </button>
            )}
            <button type="submit" className="pnl-boton pnl-boton-principal" disabled={!habilitado || !completo || enviando}>
              <Send size={15} /> {enviando ? 'Registrando...' : 'Registrar en Lycet'}
            </button>
          </div>
        </form>
      )}
    </div>
  );
}

const ICONO_CHEQUEO = { ok: CheckCircle2, aviso: AlertTriangle, error: XCircle };

function Probar({ negocio }) {
  const [chequeos, setChequeos] = useState(null);
  const [probando, setProbando] = useState(false);
  const [error, setError] = useState('');

  const probar = async () => {
    setProbando(true);
    setError('');
    try {
      setChequeos(await panelApi.probar(negocio.id));
    } catch (e) {
      setError(e.message);
    } finally {
      setProbando(false);
    }
  };

  return (
    <div>
      <p className="pnl-nota">
        Le pide a Lycet que firme una boleta de prueba (no se envía a SUNAT ni gasta números) y revisa con qué certificado la firmó.
      </p>
      <button type="button" className="pnl-boton" onClick={probar} disabled={probando}>
        <PlugZap size={15} /> {probando ? 'Probando...' : 'Probar conexión'}
      </button>
      {error && <p className="pnl-error">{error}</p>}
      {chequeos && (
        <ul className="pnl-chequeos">
          {chequeos.map((c) => {
            const Icono = ICONO_CHEQUEO[c.estado] || Circle;
            return (
              <li key={c.titulo} className={`pnl-chequeo-${c.estado}`}>
                <Icono size={18} />
                <div>
                  <strong>{c.titulo}</strong>
                  <span>{c.detalle}</span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
