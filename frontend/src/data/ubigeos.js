// Distritos del Perú con su ubigeo INEI (el que usa SUNAT en las guías de
// remisión). Una línea por provincia: "códigoProvincia|Departamento|Provincia|dd:Distrito;dd:Distrito".
// Fuente: listado INEI 2016 (github.com/ernestorivero/Ubigeo-Peru). Si falta
// un distrito creado después, el selector acepta escribir el código de 6 dígitos.
// Este archivo se carga solo cuando se abre una guía (import dinámico).
const DATOS = `0101|Amazonas|Chachapoyas|01:Chachapoyas;02:Asunción;03:Balsas;04:Cheto;05:Chiliquin;06:Chuquibamba;07:Granada;08:Huancas;09:La Jalca;10:Leimebamba;11:Levanto;12:Magdalena;13:Mariscal Castilla;14:Molinopampa;15:Montevideo;16:Olleros;17:Quinjalca;18:San Francisco de Daguas;19:San Isidro de Maino;20:Soloco;21:Sonche
0102|Amazonas|Bagua|01:Bagua;02:Aramango;03:Copallin;04:El Parco;05:Imaza;06:La Peca
0103|Amazonas|Bongará|01:Jumbilla;02:Chisquilla;03:Churuja;04:Corosha;05:Cuispes;06:Florida;07:Jazan;08:Recta;09:San Carlos;10:Shipasbamba;11:Valera;12:Yambrasbamba
0104|Amazonas|Condorcanqui|01:Nieva;02:El Cenepa;03:Río Santiago
0105|Amazonas|Luya|01:Lamud;02:Camporredondo;03:Cocabamba;04:Colcamar;05:Conila;06:Inguilpata;07:Longuita;08:Lonya Chico;09:Luya;10:Luya Viejo;11:María;12:Ocalli;13:Ocumal;14:Pisuquia;15:Providencia;16:San Cristóbal;17:San Francisco de Yeso;18:San Jerónimo;19:San Juan de Lopecancha;20:Santa Catalina;21:Santo Tomas;22:Tingo;23:Trita
0106|Amazonas|Rodríguez de Mendoza|01:San Nicolás;02:Chirimoto;03:Cochamal;04:Huambo;05:Limabamba;06:Longar;07:Mariscal Benavides;08:Milpuc;09:Omia;10:Santa Rosa;11:Totora;12:Vista Alegre
0107|Amazonas|Utcubamba|01:Bagua Grande;02:Cajaruro;03:Cumba;04:El Milagro;05:Jamalca;06:Lonya Grande;07:Yamon
0201|Áncash|Huaraz|01:Huaraz;02:Cochabamba;03:Colcabamba;04:Huanchay;05:Independencia;06:Jangas;07:La Libertad;08:Olleros;09:Pampas Grande;10:Pariacoto;11:Pira;12:Tarica
0202|Áncash|Aija|01:Aija;02:Coris;03:Huacllan;04:La Merced;05:Succha
0203|Áncash|Antonio Raymondi|01:Llamellin;02:Aczo;03:Chaccho;04:Chingas;05:Mirgas;06:San Juan de Rontoy
0204|Áncash|Asunción|01:Chacas;02:Acochaca
0205|Áncash|Bolognesi|01:Chiquian;02:Abelardo Pardo Lezameta;03:Antonio Raymondi;04:Aquia;05:Cajacay;06:Canis;07:Colquioc;08:Huallanca;09:Huasta;10:Huayllacayan;11:La Primavera;12:Mangas;13:Pacllon;14:San Miguel de Corpanqui;15:Ticllos
0206|Áncash|Carhuaz|01:Carhuaz;02:Acopampa;03:Amashca;04:Anta;05:Ataquero;06:Marcara;07:Pariahuanca;08:San Miguel de Aco;09:Shilla;10:Tinco;11:Yungar
0207|Áncash|Carlos Fermín Fitzcarrald|01:San Luis;02:San Nicolás;03:Yauya
0208|Áncash|Casma|01:Casma;02:Buena Vista Alta;03:Comandante Noel;04:Yautan
0209|Áncash|Corongo|01:Corongo;02:Aco;03:Bambas;04:Cusca;05:La Pampa;06:Yanac;07:Yupan
0210|Áncash|Huari|01:Huari;02:Anra;03:Cajay;04:Chavin de Huantar;05:Huacachi;06:Huacchis;07:Huachis;08:Huantar;09:Masin;10:Paucas;11:Ponto;12:Rahuapampa;13:Rapayan;14:San Marcos;15:San Pedro de Chana;16:Uco
0211|Áncash|Huarmey|01:Huarmey;02:Cochapeti;03:Culebras;04:Huayan;05:Malvas
0212|Áncash|Huaylas|01:Caraz;02:Huallanca;03:Huata;04:Huaylas;05:Mato;06:Pamparomas;07:Pueblo Libre;08:Santa Cruz;09:Santo Toribio;10:Yuracmarca
0213|Áncash|Mariscal Luzuriaga|01:Piscobamba;02:Casca;03:Eleazar Guzmán Barron;04:Fidel Olivas Escudero;05:Llama;06:Llumpa;07:Lucma;08:Musga
0214|Áncash|Ocros|01:Ocros;02:Acas;03:Cajamarquilla;04:Carhuapampa;05:Cochas;06:Congas;07:Llipa;08:San Cristóbal de Rajan;09:San Pedro;10:Santiago de Chilcas
0215|Áncash|Pallasca|01:Cabana;02:Bolognesi;03:Conchucos;04:Huacaschuque;05:Huandoval;06:Lacabamba;07:Llapo;08:Pallasca;09:Pampas;10:Santa Rosa;11:Tauca
0216|Áncash|Pomabamba|01:Pomabamba;02:Huayllan;03:Parobamba;04:Quinuabamba
0217|Áncash|Recuay|01:Recuay;02:Catac;03:Cotaparaco;04:Huayllapampa;05:Llacllin;06:Marca;07:Pampas Chico;08:Pararin;09:Tapacocha;10:Ticapampa
0218|Áncash|Santa|01:Chimbote;02:Cáceres del Perú;03:Coishco;04:Macate;05:Moro;06:Nepeña;07:Samanco;08:Santa;09:Nuevo Chimbote
0219|Áncash|Sihuas|01:Sihuas;02:Acobamba;03:Alfonso Ugarte;04:Cashapampa;05:Chingalpo;06:Huayllabamba;07:Quiches;08:Ragash;09:San Juan;10:Sicsibamba
0220|Áncash|Yungay|01:Yungay;02:Cascapara;03:Mancos;04:Matacoto;05:Quillo;06:Ranrahirca;07:Shupluy;08:Yanama
0301|Apurímac|Abancay|01:Abancay;02:Chacoche;03:Circa;04:Curahuasi;05:Huanipaca;06:Lambrama;07:Pichirhua;08:San Pedro de Cachora;09:Tamburco
0302|Apurímac|Andahuaylas|01:Andahuaylas;02:Andarapa;03:Chiara;04:Huancarama;05:Huancaray;06:Huayana;07:Kishuara;08:Pacobamba;09:Pacucha;10:Pampachiri;11:Pomacocha;12:San Antonio de Cachi;13:San Jerónimo;14:San Miguel de Chaccrampa;15:Santa María de Chicmo;16:Talavera;17:Tumay Huaraca;18:Turpo;19:Kaquiabamba;20:José María Arguedas
0303|Apurímac|Antabamba|01:Antabamba;02:El Oro;03:Huaquirca;04:Juan Espinoza Medrano;05:Oropesa;06:Pachaconas;07:Sabaino
0304|Apurímac|Aymaraes|01:Chalhuanca;02:Capaya;03:Caraybamba;04:Chapimarca;05:Colcabamba;06:Cotaruse;07:Ihuayllo;08:Justo Apu Sahuaraura;09:Lucre;10:Pocohuanca;11:San Juan de Chacña;12:Sañayca;13:Soraya;14:Tapairihua;15:Tintay;16:Toraya;17:Yanaca
0305|Apurímac|Cotabambas|01:Tambobamba;02:Cotabambas;03:Coyllurqui;04:Haquira;05:Mara;06:Challhuahuacho
0306|Apurímac|Chincheros|01:Chincheros;02:Anco_Huallo;03:Cocharcas;04:Huaccana;05:Ocobamba;06:Ongoy;07:Uranmarca;08:Ranracancha;09:Rocchacc;10:El Porvenir;11:Los Chankas
0307|Apurímac|Grau|01:Chuquibambilla;02:Curpahuasi;03:Gamarra;04:Huayllati;05:Mamara;06:Micaela Bastidas;07:Pataypampa;08:Progreso;09:San Antonio;10:Santa Rosa;11:Turpay;12:Vilcabamba;13:Virundo;14:Curasco
0401|Arequipa|Arequipa|01:Arequipa;02:Alto Selva Alegre;03:Cayma;04:Cerro Colorado;05:Characato;06:Chiguata;07:Jacobo Hunter;08:La Joya;09:Mariano Melgar;10:Miraflores;11:Mollebaya;12:Paucarpata;13:Pocsi;14:Polobaya;15:Quequeña;16:Sabandia;17:Sachaca;18:San Juan de Siguas;19:San Juan de Tarucani;20:Santa Isabel de Siguas;21:Santa Rita de Siguas;22:Socabaya;23:Tiabaya;24:Uchumayo;25:Vitor;26:Yanahuara;27:Yarabamba;28:Yura;29:José Luis Bustamante Y Rivero
0402|Arequipa|Camaná|01:Camaná;02:José María Quimper;03:Mariano Nicolás Valcárcel;04:Mariscal Cáceres;05:Nicolás de Pierola;06:Ocoña;07:Quilca;08:Samuel Pastor
0403|Arequipa|Caravelí|01:Caravelí;02:Acarí;03:Atico;04:Atiquipa;05:Bella Unión;06:Cahuacho;07:Chala;08:Chaparra;09:Huanuhuanu;10:Jaqui;11:Lomas;12:Quicacha;13:Yauca
0404|Arequipa|Castilla|01:Aplao;02:Andagua;03:Ayo;04:Chachas;05:Chilcaymarca;06:Choco;07:Huancarqui;08:Machaguay;09:Orcopampa;10:Pampacolca;11:Tipan;12:Uñon;13:Uraca;14:Viraco
0405|Arequipa|Caylloma|01:Chivay;02:Achoma;03:Cabanaconde;04:Callalli;05:Caylloma;06:Coporaque;07:Huambo;08:Huanca;09:Ichupampa;10:Lari;11:Lluta;12:Maca;13:Madrigal;14:San Antonio de Chuca;15:Sibayo;16:Tapay;17:Tisco;18:Tuti;19:Yanque;20:Majes
0406|Arequipa|Condesuyos|01:Chuquibamba;02:Andaray;03:Cayarani;04:Chichas;05:Iray;06:Río Grande;07:Salamanca;08:Yanaquihua
0407|Arequipa|Islay|01:Mollendo;02:Cocachacra;03:Dean Valdivia;04:Islay;05:Mejia;06:Punta de Bombón
0408|Arequipa|La Uniòn|01:Cotahuasi;02:Alca;03:Charcana;04:Huaynacotas;05:Pampamarca;06:Puyca;07:Quechualla;08:Sayla;09:Tauria;10:Tomepampa;11:Toro
0501|Ayacucho|Huamanga|01:Ayacucho;02:Acocro;03:Acos Vinchos;04:Carmen Alto;05:Chiara;06:Ocros;07:Pacaycasa;08:Quinua;09:San José de Ticllas;10:San Juan Bautista;11:Santiago de Pischa;12:Socos;13:Tambillo;14:Vinchos;15:Jesús Nazareno;16:Andrés Avelino Cáceres Dorregaray
0502|Ayacucho|Cangallo|01:Cangallo;02:Chuschi;03:Los Morochucos;04:María Parado de Bellido;05:Paras;06:Totos
0503|Ayacucho|Huanca Sancos|01:Sancos;02:Carapo;03:Sacsamarca;04:Santiago de Lucanamarca
0504|Ayacucho|Huanta|01:Huanta;02:Ayahuanco;03:Huamanguilla;04:Iguain;05:Luricocha;06:Santillana;07:Sivia;08:Llochegua;09:Canayre;10:Uchuraccay;11:Pucacolpa;12:Chaca
0505|Ayacucho|La Mar|01:San Miguel;02:Anco;03:Ayna;04:Chilcas;05:Chungui;06:Luis Carranza;07:Santa Rosa;08:Tambo;09:Samugari;10:Anchihuay;11:Oronccoy
0506|Ayacucho|Lucanas|01:Puquio;02:Aucara;03:Cabana;04:Carmen Salcedo;05:Chaviña;06:Chipao;07:Huac-Huas;08:Laramate;09:Leoncio Prado;10:Llauta;11:Lucanas;12:Ocaña;13:Otoca;14:Saisa;15:San Cristóbal;16:San Juan;17:San Pedro;18:San Pedro de Palco;19:Sancos;20:Santa Ana de Huaycahuacho;21:Santa Lucia
0507|Ayacucho|Parinacochas|01:Coracora;02:Chumpi;03:Coronel Castañeda;04:Pacapausa;05:Pullo;06:Puyusca;07:San Francisco de Ravacayco;08:Upahuacho
0508|Ayacucho|Pàucar del Sara Sara|01:Pausa;02:Colta;03:Corculla;04:Lampa;05:Marcabamba;06:Oyolo;07:Pararca;08:San Javier de Alpabamba;09:San José de Ushua;10:Sara Sara
0509|Ayacucho|Sucre|01:Querobamba;02:Belén;03:Chalcos;04:Chilcayoc;05:Huacaña;06:Morcolla;07:Paico;08:San Pedro de Larcay;09:San Salvador de Quije;10:Santiago de Paucaray;11:Soras
0510|Ayacucho|Víctor Fajardo|01:Huancapi;02:Alcamenca;03:Apongo;04:Asquipata;05:Canaria;06:Cayara;07:Colca;08:Huamanquiquia;09:Huancaraylla;10:Hualla;11:Sarhua;12:Vilcanchos
0511|Ayacucho|Vilcas Huamán|01:Vilcas Huaman;02:Accomarca;03:Carhuanca;04:Concepción;05:Huambalpa;06:Independencia;07:Saurama;08:Vischongo
0601|Cajamarca|Cajamarca|01:Cajamarca;02:Asunción;03:Chetilla;04:Cospan;05:Encañada;06:Jesús;07:Llacanora;08:Los Baños del Inca;09:Magdalena;10:Matara;11:Namora;12:San Juan
0602|Cajamarca|Cajabamba|01:Cajabamba;02:Cachachi;03:Condebamba;04:Sitacocha
0603|Cajamarca|Celendín|01:Celendín;02:Chumuch;03:Cortegana;04:Huasmin;05:Jorge Chávez;06:José Gálvez;07:Miguel Iglesias;08:Oxamarca;09:Sorochuco;10:Sucre;11:Utco;12:La Libertad de Pallan
0604|Cajamarca|Chota|01:Chota;02:Anguia;03:Chadin;04:Chiguirip;05:Chimban;06:Choropampa;07:Cochabamba;08:Conchan;09:Huambos;10:Lajas;11:Llama;12:Miracosta;13:Paccha;14:Pion;15:Querocoto;16:San Juan de Licupis;17:Tacabamba;18:Tocmoche;19:Chalamarca
0605|Cajamarca|Contumazá|01:Contumaza;02:Chilete;03:Cupisnique;04:Guzmango;05:San Benito;06:Santa Cruz de Toledo;07:Tantarica;08:Yonan
0606|Cajamarca|Cutervo|01:Cutervo;02:Callayuc;03:Choros;04:Cujillo;05:La Ramada;06:Pimpingos;07:Querocotillo;08:San Andrés de Cutervo;09:San Juan de Cutervo;10:San Luis de Lucma;11:Santa Cruz;12:Santo Domingo de la Capilla;13:Santo Tomas;14:Socota;15:Toribio Casanova
0607|Cajamarca|Hualgayoc|01:Bambamarca;02:Chugur;03:Hualgayoc
0608|Cajamarca|Jaén|01:Jaén;02:Bellavista;03:Chontali;04:Colasay;05:Huabal;06:Las Pirias;07:Pomahuaca;08:Pucara;09:Sallique;10:San Felipe;11:San José del Alto;12:Santa Rosa
0609|Cajamarca|San Ignacio|01:San Ignacio;02:Chirinos;03:Huarango;04:La Coipa;05:Namballe;06:San José de Lourdes;07:Tabaconas
0610|Cajamarca|San Marcos|01:Pedro Gálvez;02:Chancay;03:Eduardo Villanueva;04:Gregorio Pita;05:Ichocan;06:José Manuel Quiroz;07:José Sabogal
0611|Cajamarca|San Miguel|01:San Miguel;02:Bolívar;03:Calquis;04:Catilluc;05:El Prado;06:La Florida;07:Llapa;08:Nanchoc;09:Niepos;10:San Gregorio;11:San Silvestre de Cochan;12:Tongod;13:Unión Agua Blanca
0612|Cajamarca|San Pablo|01:San Pablo;02:San Bernardino;03:San Luis;04:Tumbaden
0613|Cajamarca|Santa Cruz|01:Santa Cruz;02:Andabamba;03:Catache;04:Chancaybaños;05:La Esperanza;06:Ninabamba;07:Pulan;08:Saucepampa;09:Sexi;10:Uticyacu;11:Yauyucan
0701|Callao|Callao|01:Callao;02:Bellavista;03:Carmen de la Legua Reynoso;04:La Perla;05:La Punta;06:Ventanilla;07:Mi Perú
0801|Cusco|Cusco|01:Cusco;02:Ccorca;03:Poroy;04:San Jerónimo;05:San Sebastian;06:Santiago;07:Saylla;08:Wanchaq
0802|Cusco|Acomayo|01:Acomayo;02:Acopia;03:Acos;04:Mosoc Llacta;05:Pomacanchi;06:Rondocan;07:Sangarara
0803|Cusco|Anta|01:Anta;02:Ancahuasi;03:Cachimayo;04:Chinchaypujio;05:Huarocondo;06:Limatambo;07:Mollepata;08:Pucyura;09:Zurite
0804|Cusco|Calca|01:Calca;02:Coya;03:Lamay;04:Lares;05:Pisac;06:San Salvador;07:Taray;08:Yanatile
0805|Cusco|Canas|01:Yanaoca;02:Checca;03:Kunturkanki;04:Langui;05:Layo;06:Pampamarca;07:Quehue;08:Tupac Amaru
0806|Cusco|Canchis|01:Sicuani;02:Checacupe;03:Combapata;04:Marangani;05:Pitumarca;06:San Pablo;07:San Pedro;08:Tinta
0807|Cusco|Chumbivilcas|01:Santo Tomas;02:Capacmarca;03:Chamaca;04:Colquemarca;05:Livitaca;06:Llusco;07:Quiñota;08:Velille
0808|Cusco|Espinar|01:Espinar;02:Condoroma;03:Coporaque;04:Ocoruro;05:Pallpata;06:Pichigua;07:Suyckutambo;08:Alto Pichigua
0809|Cusco|La Convención|01:Santa Ana;02:Echarate;03:Huayopata;04:Maranura;05:Ocobamba;06:Quellouno;07:Kimbiri;08:Santa Teresa;09:Vilcabamba;10:Pichari;11:Inkawasi;12:Villa Virgen;13:Villa Kintiarina;14:Megantoni
0810|Cusco|Paruro|01:Paruro;02:Accha;03:Ccapi;04:Colcha;05:Huanoquite;06:Omachaç;07:Paccaritambo;08:Pillpinto;09:Yaurisque
0811|Cusco|Paucartambo|01:Paucartambo;02:Caicay;03:Challabamba;04:Colquepata;05:Huancarani;06:Kosñipata
0812|Cusco|Quispicanchi|01:Urcos;02:Andahuaylillas;03:Camanti;04:Ccarhuayo;05:Ccatca;06:Cusipata;07:Huaro;08:Lucre;09:Marcapata;10:Ocongate;11:Oropesa;12:Quiquijana
0813|Cusco|Urubamba|01:Urubamba;02:Chinchero;03:Huayllabamba;04:Machupicchu;05:Maras;06:Ollantaytambo;07:Yucay
0901|Huancavelica|Huancavelica|01:Huancavelica;02:Acobambilla;03:Acoria;04:Conayca;05:Cuenca;06:Huachocolpa;07:Huayllahuara;08:Izcuchaca;09:Laria;10:Manta;11:Mariscal Cáceres;12:Moya;13:Nuevo Occoro;14:Palca;15:Pilchaca;16:Vilca;17:Yauli;18:Ascensión;19:Huando
0902|Huancavelica|Acobamba|01:Acobamba;02:Andabamba;03:Anta;04:Caja;05:Marcas;06:Paucara;07:Pomacocha;08:Rosario
0903|Huancavelica|Angaraes|01:Lircay;02:Anchonga;03:Callanmarca;04:Ccochaccasa;05:Chincho;06:Congalla;07:Huanca-Huanca;08:Huayllay Grande;09:Julcamarca;10:San Antonio de Antaparco;11:Santo Tomas de Pata;12:Secclla
0904|Huancavelica|Castrovirreyna|01:Castrovirreyna;02:Arma;03:Aurahua;04:Capillas;05:Chupamarca;06:Cocas;07:Huachos;08:Huamatambo;09:Mollepampa;10:San Juan;11:Santa Ana;12:Tantara;13:Ticrapo
0905|Huancavelica|Churcampa|01:Churcampa;02:Anco;03:Chinchihuasi;04:El Carmen;05:La Merced;06:Locroja;07:Paucarbamba;08:San Miguel de Mayocc;09:San Pedro de Coris;10:Pachamarca;11:Cosme
0906|Huancavelica|Huaytará|01:Huaytara;02:Ayavi;03:Córdova;04:Huayacundo Arma;05:Laramarca;06:Ocoyo;07:Pilpichaca;08:Querco;09:Quito-Arma;10:San Antonio de Cusicancha;11:San Francisco de Sangayaico;12:San Isidro;13:Santiago de Chocorvos;14:Santiago de Quirahuara;15:Santo Domingo de Capillas;16:Tambo
0907|Huancavelica|Tayacaja|01:Pampas;02:Acostambo;03:Acraquia;04:Ahuaycha;05:Colcabamba;06:Daniel Hernández;07:Huachocolpa;09:Huaribamba;10:Ñahuimpuquio;11:Pazos;13:Quishuar;14:Salcabamba;15:Salcahuasi;16:San Marcos de Rocchac;17:Surcubamba;18:Tintay Puncu;19:Quichuas;20:Andaymarca;21:Roble;22:Pichos;23:Santiago de Tucuma
1001|Huánuco|Huánuco|01:Huanuco;02:Amarilis;03:Chinchao;04:Churubamba;05:Margos;06:Quisqui (Kichki);07:San Francisco de Cayran;08:San Pedro de Chaulan;09:Santa María del Valle;10:Yarumayo;11:Pillco Marca;12:Yacus;13:San Pablo de Pillao
1002|Huánuco|Ambo|01:Ambo;02:Cayna;03:Colpas;04:Conchamarca;05:Huacar;06:San Francisco;07:San Rafael;08:Tomay Kichwa
1003|Huánuco|Dos de Mayo|01:La Unión;07:Chuquis;11:Marías;13:Pachas;16:Quivilla;17:Ripan;21:Shunqui;22:Sillapata;23:Yanas
1004|Huánuco|Huacaybamba|01:Huacaybamba;02:Canchabamba;03:Cochabamba;04:Pinra
1005|Huánuco|Huamalíes|01:Llata;02:Arancay;03:Chavín de Pariarca;04:Jacas Grande;05:Jircan;06:Miraflores;07:Monzón;08:Punchao;09:Puños;10:Singa;11:Tantamayo
1006|Huánuco|Leoncio Prado|01:Rupa-Rupa;02:Daniel Alomía Robles;03:Hermílio Valdizan;04:José Crespo y Castillo;05:Luyando;06:Mariano Damaso Beraun;07:Pucayacu;08:Castillo Grande;09:Pueblo Nuevo;10:Santo Domingo de Anda
1007|Huánuco|Marañón|01:Huacrachuco;02:Cholon;03:San Buenaventura;04:La Morada;05:Santa Rosa de Alto Yanajanca
1008|Huánuco|Pachitea|01:Panao;02:Chaglla;03:Molino;04:Umari
1009|Huánuco|Puerto Inca|01:Puerto Inca;02:Codo del Pozuzo;03:Honoria;04:Tournavista;05:Yuyapichis
1010|Huánuco|Lauricocha|01:Jesús;02:Baños;03:Jivia;04:Queropalca;05:Rondos;06:San Francisco de Asís;07:San Miguel de Cauri
1011|Huánuco|Yarowilca|01:Chavinillo;02:Cahuac;03:Chacabamba;04:Aparicio Pomares;05:Jacas Chico;06:Obas;07:Pampamarca;08:Choras
1101|Ica|Ica|01:Ica;02:La Tinguiña;03:Los Aquijes;04:Ocucaje;05:Pachacutec;06:Parcona;07:Pueblo Nuevo;08:Salas;09:San José de Los Molinos;10:San Juan Bautista;11:Santiago;12:Subtanjalla;13:Tate;14:Yauca del Rosario
1102|Ica|Chincha|01:Chincha Alta;02:Alto Laran;03:Chavin;04:Chincha Baja;05:El Carmen;06:Grocio Prado;07:Pueblo Nuevo;08:San Juan de Yanac;09:San Pedro de Huacarpana;10:Sunampe;11:Tambo de Mora
1103|Ica|Nasca|01:Nasca;02:Changuillo;03:El Ingenio;04:Marcona;05:Vista Alegre
1104|Ica|Palpa|01:Palpa;02:Llipata;03:Río Grande;04:Santa Cruz;05:Tibillo
1105|Ica|Pisco|01:Pisco;02:Huancano;03:Humay;04:Independencia;05:Paracas;06:San Andrés;07:San Clemente;08:Tupac Amaru Inca
1201|Junín|Huancayo|01:Huancayo;04:Carhuacallanga;05:Chacapampa;06:Chicche;07:Chilca;08:Chongos Alto;11:Chupuro;12:Colca;13:Cullhuas;14:El Tambo;16:Huacrapuquio;17:Hualhuas;19:Huancan;20:Huasicancha;21:Huayucachi;22:Ingenio;24:Pariahuanca;25:Pilcomayo;26:Pucara;27:Quichuay;28:Quilcas;29:San Agustín;30:San Jerónimo de Tunan;32:Saño;33:Sapallanga;34:Sicaya;35:Santo Domingo de Acobamba;36:Viques
1202|Junín|Concepción|01:Concepción;02:Aco;03:Andamarca;04:Chambara;05:Cochas;06:Comas;07:Heroínas Toledo;08:Manzanares;09:Mariscal Castilla;10:Matahuasi;11:Mito;12:Nueve de Julio;13:Orcotuna;14:San José de Quero;15:Santa Rosa de Ocopa
1203|Junín|Chanchamayo|01:Chanchamayo;02:Perene;03:Pichanaqui;04:San Luis de Shuaro;05:San Ramón;06:Vitoc
1204|Junín|Jauja|01:Jauja;02:Acolla;03:Apata;04:Ataura;05:Canchayllo;06:Curicaca;07:El Mantaro;08:Huamali;09:Huaripampa;10:Huertas;11:Janjaillo;12:Julcán;13:Leonor Ordóñez;14:Llocllapampa;15:Marco;16:Masma;17:Masma Chicche;18:Molinos;19:Monobamba;20:Muqui;21:Muquiyauyo;22:Paca;23:Paccha;24:Pancan;25:Parco;26:Pomacancha;27:Ricran;28:San Lorenzo;29:San Pedro de Chunan;30:Sausa;31:Sincos;32:Tunan Marca;33:Yauli;34:Yauyos
1205|Junín|Junín|01:Junin;02:Carhuamayo;03:Ondores;04:Ulcumayo
1206|Junín|Satipo|01:Satipo;02:Coviriali;03:Llaylla;04:Mazamari;05:Pampa Hermosa;06:Pangoa;07:Río Negro;08:Río Tambo;09:Vizcatan del Ene
1207|Junín|Tarma|01:Tarma;02:Acobamba;03:Huaricolca;04:Huasahuasi;05:La Unión;06:Palca;07:Palcamayo;08:San Pedro de Cajas;09:Tapo
1208|Junín|Yauli|01:La Oroya;02:Chacapalpa;03:Huay-Huay;04:Marcapomacocha;05:Morococha;06:Paccha;07:Santa Bárbara de Carhuacayan;08:Santa Rosa de Sacco;09:Suitucancha;10:Yauli
1209|Junín|Chupaca|01:Chupaca;02:Ahuac;03:Chongos Bajo;04:Huachac;05:Huamancaca Chico;06:San Juan de Iscos;07:San Juan de Jarpa;08:Tres de Diciembre;09:Yanacancha
1301|La Libertad|Trujillo|01:Trujillo;02:El Porvenir;03:Florencia de Mora;04:Huanchaco;05:La Esperanza;06:Laredo;07:Moche;08:Poroto;09:Salaverry;10:Simbal;11:Victor Larco Herrera
1302|La Libertad|Ascope|01:Ascope;02:Chicama;03:Chocope;04:Magdalena de Cao;05:Paijan;06:Rázuri;07:Santiago de Cao;08:Casa Grande
1303|La Libertad|Bolívar|01:Bolívar;02:Bambamarca;03:Condormarca;04:Longotea;05:Uchumarca;06:Ucuncha
1304|La Libertad|Chepén|01:Chepen;02:Pacanga;03:Pueblo Nuevo
1305|La Libertad|Julcán|01:Julcan;02:Calamarca;03:Carabamba;04:Huaso
1306|La Libertad|Otuzco|01:Otuzco;02:Agallpampa;04:Charat;05:Huaranchal;06:La Cuesta;08:Mache;10:Paranday;11:Salpo;13:Sinsicap;14:Usquil
1307|La Libertad|Pacasmayo|01:San Pedro de Lloc;02:Guadalupe;03:Jequetepeque;04:Pacasmayo;05:San José
1308|La Libertad|Pataz|01:Tayabamba;02:Buldibuyo;03:Chillia;04:Huancaspata;05:Huaylillas;06:Huayo;07:Ongon;08:Parcoy;09:Pataz;10:Pias;11:Santiago de Challas;12:Taurija;13:Urpay
1309|La Libertad|Sánchez Carrión|01:Huamachuco;02:Chugay;03:Cochorco;04:Curgos;05:Marcabal;06:Sanagoran;07:Sarin;08:Sartimbamba
1310|La Libertad|Santiago de Chuco|01:Santiago de Chuco;02:Angasmarca;03:Cachicadan;04:Mollebamba;05:Mollepata;06:Quiruvilca;07:Santa Cruz de Chuca;08:Sitabamba
1311|La Libertad|Gran Chimú|01:Cascas;02:Lucma;03:Marmot;04:Sayapullo
1312|La Libertad|Virú|01:Viru;02:Chao;03:Guadalupito
1401|Lambayeque|Chiclayo|01:Chiclayo;02:Chongoyape;03:Eten;04:Eten Puerto;05:José Leonardo Ortiz;06:La Victoria;07:Lagunas;08:Monsefu;09:Nueva Arica;10:Oyotun;11:Picsi;12:Pimentel;13:Reque;14:Santa Rosa;15:Saña;16:Cayalti;17:Patapo;18:Pomalca;19:Pucala;20:Tuman
1402|Lambayeque|Ferreñafe|01:Ferreñafe;02:Cañaris;03:Incahuasi;04:Manuel Antonio Mesones Muro;05:Pitipo;06:Pueblo Nuevo
1403|Lambayeque|Lambayeque|01:Lambayeque;02:Chochope;03:Illimo;04:Jayanca;05:Mochumi;06:Morrope;07:Motupe;08:Olmos;09:Pacora;10:Salas;11:San José;12:Tucume
1501|Lima|Lima|01:Lima;02:Ancón;03:Ate;04:Barranco;05:Breña;06:Carabayllo;07:Chaclacayo;08:Chorrillos;09:Cieneguilla;10:Comas;11:El Agustino;12:Independencia;13:Jesús María;14:La Molina;15:La Victoria;16:Lince;17:Los Olivos;18:Lurigancho;19:Lurin;20:Magdalena del Mar;21:Pueblo Libre;22:Miraflores;23:Pachacamac;24:Pucusana;25:Puente Piedra;26:Punta Hermosa;27:Punta Negra;28:Rímac;29:San Bartolo;30:San Borja;31:San Isidro;32:San Juan de Lurigancho;33:San Juan de Miraflores;34:San Luis;35:San Martín de Porres;36:San Miguel;37:Santa Anita;38:Santa María del Mar;39:Santa Rosa;40:Santiago de Surco;41:Surquillo;42:Villa El Salvador;43:Villa María del Triunfo
1502|Lima|Barranca|01:Barranca;02:Paramonga;03:Pativilca;04:Supe;05:Supe Puerto
1503|Lima|Cajatambo|01:Cajatambo;02:Copa;03:Gorgor;04:Huancapon;05:Manas
1504|Lima|Canta|01:Canta;02:Arahuay;03:Huamantanga;04:Huaros;05:Lachaqui;06:San Buenaventura;07:Santa Rosa de Quives
1505|Lima|Cañete|01:San Vicente de Cañete;02:Asia;03:Calango;04:Cerro Azul;05:Chilca;06:Coayllo;07:Imperial;08:Lunahuana;09:Mala;10:Nuevo Imperial;11:Pacaran;12:Quilmana;13:San Antonio;14:San Luis;15:Santa Cruz de Flores;16:Zúñiga
1506|Lima|Huaral|01:Huaral;02:Atavillos Alto;03:Atavillos Bajo;04:Aucallama;05:Chancay;06:Ihuari;07:Lampian;08:Pacaraos;09:San Miguel de Acos;10:Santa Cruz de Andamarca;11:Sumbilca;12:Veintisiete de Noviembre
1507|Lima|Huarochirí|01:Matucana;02:Antioquia;03:Callahuanca;04:Carampoma;05:Chicla;06:Cuenca;07:Huachupampa;08:Huanza;09:Huarochiri;10:Lahuaytambo;11:Langa;12:Laraos;13:Mariatana;14:Ricardo Palma;15:San Andrés de Tupicocha;16:San Antonio;17:San Bartolomé;18:San Damian;19:San Juan de Iris;20:San Juan de Tantaranche;21:San Lorenzo de Quinti;22:San Mateo;23:San Mateo de Otao;24:San Pedro de Casta;25:San Pedro de Huancayre;26:Sangallaya;27:Santa Cruz de Cocachacra;28:Santa Eulalia;29:Santiago de Anchucaya;30:Santiago de Tuna;31:Santo Domingo de Los Olleros;32:Surco
1508|Lima|Huaura|01:Huacho;02:Ambar;03:Caleta de Carquin;04:Checras;05:Hualmay;06:Huaura;07:Leoncio Prado;08:Paccho;09:Santa Leonor;10:Santa María;11:Sayan;12:Vegueta
1509|Lima|Oyón|01:Oyon;02:Andajes;03:Caujul;04:Cochamarca;05:Navan;06:Pachangara
1510|Lima|Yauyos|01:Yauyos;02:Alis;03:Allauca;04:Ayaviri;05:Azángaro;06:Cacra;07:Carania;08:Catahuasi;09:Chocos;10:Cochas;11:Colonia;12:Hongos;13:Huampara;14:Huancaya;15:Huangascar;16:Huantan;17:Huañec;18:Laraos;19:Lincha;20:Madean;21:Miraflores;22:Omas;23:Putinza;24:Quinches;25:Quinocay;26:San Joaquín;27:San Pedro de Pilas;28:Tanta;29:Tauripampa;30:Tomas;31:Tupe;32:Viñac;33:Vitis
1601|Loreto|Maynas|01:Iquitos;02:Alto Nanay;03:Fernando Lores;04:Indiana;05:Las Amazonas;06:Mazan;07:Napo;08:Punchana;10:Torres Causana;12:Belén;13:San Juan Bautista
1602|Loreto|Alto Amazonas|01:Yurimaguas;02:Balsapuerto;05:Jeberos;06:Lagunas;10:Santa Cruz;11:Teniente Cesar López Rojas
1603|Loreto|Loreto|01:Nauta;02:Parinari;03:Tigre;04:Trompeteros;05:Urarinas
1604|Loreto|Mariscal Ramón Castilla|01:Ramón Castilla;02:Pebas;03:Yavari;04:San Pablo
1605|Loreto|Requena|01:Requena;02:Alto Tapiche;03:Capelo;04:Emilio San Martín;05:Maquia;06:Puinahua;07:Saquena;08:Soplin;09:Tapiche;10:Jenaro Herrera;11:Yaquerana
1606|Loreto|Ucayali|01:Contamana;02:Inahuaya;03:Padre Márquez;04:Pampa Hermosa;05:Sarayacu;06:Vargas Guerra
1607|Loreto|Datem del Marañón|01:Barranca;02:Cahuapanas;03:Manseriche;04:Morona;05:Pastaza;06:Andoas
1608|Loreto|Putumayo|01:Putumayo;02:Rosa Panduro;03:Teniente Manuel Clavero;04:Yaguas
1701|Madre de Dios|Tambopata|01:Tambopata;02:Inambari;03:Las Piedras;04:Laberinto
1702|Madre de Dios|Manu|01:Manu;02:Fitzcarrald;03:Madre de Dios;04:Huepetuhe
1703|Madre de Dios|Tahuamanu|01:Iñapari;02:Iberia;03:Tahuamanu
1801|Moquegua|Mariscal Nieto|01:Moquegua;02:Carumas;03:Cuchumbaya;04:Samegua;05:San Cristóbal;06:Torata
1802|Moquegua|General Sánchez Cerro|01:Omate;02:Chojata;03:Coalaque;04:Ichuña;05:La Capilla;06:Lloque;07:Matalaque;08:Puquina;09:Quinistaquillas;10:Ubinas;11:Yunga
1803|Moquegua|Ilo|01:Ilo;02:El Algarrobal;03:Pacocha
1901|Pasco|Pasco|01:Chaupimarca;02:Huachon;03:Huariaca;04:Huayllay;05:Ninacaca;06:Pallanchacra;07:Paucartambo;08:San Francisco de Asís de Yarusyacan;09:Simon Bolívar;10:Ticlacayan;11:Tinyahuarco;12:Vicco;13:Yanacancha
1902|Pasco|Daniel Alcides Carrión|01:Yanahuanca;02:Chacayan;03:Goyllarisquizga;04:Paucar;05:San Pedro de Pillao;06:Santa Ana de Tusi;07:Tapuc;08:Vilcabamba
1903|Pasco|Oxapampa|01:Oxapampa;02:Chontabamba;03:Huancabamba;04:Palcazu;05:Pozuzo;06:Puerto Bermúdez;07:Villa Rica;08:Constitución
2001|Piura|Piura|01:Piura;04:Castilla;05:Catacaos;07:Cura Mori;08:El Tallan;09:La Arena;10:La Unión;11:Las Lomas;14:Tambo Grande;15:Veintiseis de Octubre
2002|Piura|Ayabaca|01:Ayabaca;02:Frias;03:Jilili;04:Lagunas;05:Montero;06:Pacaipampa;07:Paimas;08:Sapillica;09:Sicchez;10:Suyo
2003|Piura|Huancabamba|01:Huancabamba;02:Canchaque;03:El Carmen de la Frontera;04:Huarmaca;05:Lalaquiz;06:San Miguel de El Faique;07:Sondor;08:Sondorillo
2004|Piura|Morropón|01:Chulucanas;02:Buenos Aires;03:Chalaco;04:La Matanza;05:Morropon;06:Salitral;07:San Juan de Bigote;08:Santa Catalina de Mossa;09:Santo Domingo;10:Yamango
2005|Piura|Paita|01:Paita;02:Amotape;03:Arenal;04:Colan;05:La Huaca;06:Tamarindo;07:Vichayal
2006|Piura|Sullana|01:Sullana;02:Bellavista;03:Ignacio Escudero;04:Lancones;05:Marcavelica;06:Miguel Checa;07:Querecotillo;08:Salitral
2007|Piura|Talara|01:Pariñas;02:El Alto;03:La Brea;04:Lobitos;05:Los Organos;06:Mancora
2008|Piura|Sechura|01:Sechura;02:Bellavista de la Unión;03:Bernal;04:Cristo Nos Valga;05:Vice;06:Rinconada Llicuar
2101|Puno|Puno|01:Puno;02:Acora;03:Amantani;04:Atuncolla;05:Capachica;06:Chucuito;07:Coata;08:Huata;09:Mañazo;10:Paucarcolla;11:Pichacani;12:Plateria;13:San Antonio;14:Tiquillaca;15:Vilque
2102|Puno|Azángaro|01:Azángaro;02:Achaya;03:Arapa;04:Asillo;05:Caminaca;06:Chupa;07:José Domingo Choquehuanca;08:Muñani;09:Potoni;10:Saman;11:San Anton;12:San José;13:San Juan de Salinas;14:Santiago de Pupuja;15:Tirapata
2103|Puno|Carabaya|01:Macusani;02:Ajoyani;03:Ayapata;04:Coasa;05:Corani;06:Crucero;07:Ituata;08:Ollachea;09:San Gaban;10:Usicayos
2104|Puno|Chucuito|01:Juli;02:Desaguadero;03:Huacullani;04:Kelluyo;05:Pisacoma;06:Pomata;07:Zepita
2105|Puno|El Collao|01:Ilave;02:Capazo;03:Pilcuyo;04:Santa Rosa;05:Conduriri
2106|Puno|Huancané|01:Huancane;02:Cojata;03:Huatasani;04:Inchupalla;05:Pusi;06:Rosaspata;07:Taraco;08:Vilque Chico
2107|Puno|Lampa|01:Lampa;02:Cabanilla;03:Calapuja;04:Nicasio;05:Ocuviri;06:Palca;07:Paratia;08:Pucara;09:Santa Lucia;10:Vilavila
2108|Puno|Melgar|01:Ayaviri;02:Antauta;03:Cupi;04:Llalli;05:Macari;06:Nuñoa;07:Orurillo;08:Santa Rosa;09:Umachiri
2109|Puno|Moho|01:Moho;02:Conima;03:Huayrapata;04:Tilali
2110|Puno|San Antonio de Putina|01:Putina;02:Ananea;03:Pedro Vilca Apaza;04:Quilcapuncu;05:Sina
2111|Puno|San Román|01:Juliaca;02:Cabana;03:Cabanillas;04:Caracoto;05:San Miguel
2112|Puno|Sandia|01:Sandia;02:Cuyocuyo;03:Limbani;04:Patambuco;05:Phara;06:Quiaca;07:San Juan del Oro;08:Yanahuaya;09:Alto Inambari;10:San Pedro de Putina Punco
2113|Puno|Yunguyo|01:Yunguyo;02:Anapia;03:Copani;04:Cuturapi;05:Ollaraya;06:Tinicachi;07:Unicachi
2201|San Martín|Moyobamba|01:Moyobamba;02:Calzada;03:Habana;04:Jepelacio;05:Soritor;06:Yantalo
2202|San Martín|Bellavista|01:Bellavista;02:Alto Biavo;03:Bajo Biavo;04:Huallaga;05:San Pablo;06:San Rafael
2203|San Martín|El Dorado|01:San José de Sisa;02:Agua Blanca;03:San Martín;04:Santa Rosa;05:Shatoja
2204|San Martín|Huallaga|01:Saposoa;02:Alto Saposoa;03:El Eslabón;04:Piscoyacu;05:Sacanche;06:Tingo de Saposoa
2205|San Martín|Lamas|01:Lamas;02:Alonso de Alvarado;03:Barranquita;04:Caynarachi;05:Cuñumbuqui;06:Pinto Recodo;07:Rumisapa;08:San Roque de Cumbaza;09:Shanao;10:Tabalosos;11:Zapatero
2206|San Martín|Mariscal Cáceres|01:Juanjuí;02:Campanilla;03:Huicungo;04:Pachiza;05:Pajarillo
2207|San Martín|Picota|01:Picota;02:Buenos Aires;03:Caspisapa;04:Pilluana;05:Pucacaca;06:San Cristóbal;07:San Hilarión;08:Shamboyacu;09:Tingo de Ponasa;10:Tres Unidos
2208|San Martín|Rioja|01:Rioja;02:Awajun;03:Elías Soplin Vargas;04:Nueva Cajamarca;05:Pardo Miguel;06:Posic;07:San Fernando;08:Yorongos;09:Yuracyacu
2209|San Martín|San Martín|01:Tarapoto;02:Alberto Leveau;03:Cacatachi;04:Chazuta;05:Chipurana;06:El Porvenir;07:Huimbayoc;08:Juan Guerra;09:La Banda de Shilcayo;10:Morales;11:Papaplaya;12:San Antonio;13:Sauce;14:Shapaja
2210|San Martín|Tocache|01:Tocache;02:Nuevo Progreso;03:Polvora;04:Shunte;05:Uchiza
2301|Tacna|Tacna|01:Tacna;02:Alto de la Alianza;03:Calana;04:Ciudad Nueva;05:Inclan;06:Pachia;07:Palca;08:Pocollay;09:Sama;10:Coronel Gregorio Albarracín Lanchipa;11:La Yarada los Palos
2302|Tacna|Candarave|01:Candarave;02:Cairani;03:Camilaca;04:Curibaya;05:Huanuara;06:Quilahuani
2303|Tacna|Jorge Basadre|01:Locumba;02:Ilabaya;03:Ite
2304|Tacna|Tarata|01:Tarata;02:Héroes Albarracín;03:Estique;04:Estique-Pampa;05:Sitajara;06:Susapaya;07:Tarucachi;08:Ticaco
2401|Tumbes|Tumbes|01:Tumbes;02:Corrales;03:La Cruz;04:Pampas de Hospital;05:San Jacinto;06:San Juan de la Virgen
2402|Tumbes|Contralmirante Villar|01:Zorritos;02:Casitas;03:Canoas de Punta Sal
2403|Tumbes|Zarumilla|01:Zarumilla;02:Aguas Verdes;03:Matapalo;04:Papayal
2501|Ucayali|Coronel Portillo|01:Calleria;02:Campoverde;03:Iparia;04:Masisea;05:Yarinacocha;06:Nueva Requena;07:Manantay
2502|Ucayali|Atalaya|01:Raymondi;02:Sepahua;03:Tahuania;04:Yurua
2503|Ucayali|Padre Abad|01:Padre Abad;02:Irazola;03:Curimana;04:Neshuya;05:Alexander Von Humboldt
2504|Ucayali|Purús|01:Purus`;

/** [{ ubigeo: '080105', distrito, provincia, departamento }] */
export const UBIGEOS = DATOS.split('\n').flatMap((linea) => {
  const [provinciaId, departamento, provincia, distritos] = linea.split('|');
  return distritos.split(';').map((d) => {
    const [codigo, distrito] = d.split(':');
    return { ubigeo: provinciaId + codigo, distrito, provincia, departamento };
  });
});
